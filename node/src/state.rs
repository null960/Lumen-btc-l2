use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use chrono::Utc;
use sha2::{Sha256, Digest};

// ── Constants ─────────────────────────────────────────────────────────────────

pub const DUST_LIMIT:            u64 = 546;
pub const FAUCET_AMOUNT:         u64 = 10_000;
pub const FAUCET_COOLDOWN_SEC:   i64 = 86_400;  // 24h per address
pub const CHALLENGE_WINDOW_SEC:  i64 = 86_400;  // 24h PegOut challenge
pub const MAX_HISTORY:           usize = 10_000; // rolling window

pub type Lsat = u64;

// ── Data Structures ───────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppInfo {
    pub app_id:       String,
    pub app_name:     String,
    pub owner:        String,
    pub description:  String,
    pub website:      Option<String>,
    pub token_name:   String,
    pub rate_per_lsat: u64,
    pub created_at:   i64,
    pub lsat_collected: Lsat,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum WithdrawalStatus {
    Pending,
    Completed(String), // btc txid
    Failed(String),    // error reason
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WithdrawalRequest {
    pub id:                 String,
    pub user:               String,
    pub btc_address:        String,
    pub amount:             Lsat,
    pub status:             WithdrawalStatus,
    pub created_at:         i64,
    pub challenge_deadline: i64,
    pub retry_count:        u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TxRecord {
    pub tx_type:   String,
    pub token:     String,
    pub from:      String,
    pub to:        String,
    pub amount:    u64,
    pub txid:      String,
    pub timestamp: i64,
    pub memo:      Option<String>,
    pub btc_txid:  Option<String>,
    pub status:    String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct NetworkStats {
    pub total_pegins:       u64,
    pub total_pegouts:      u64,
    pub total_transfers:    u64,
    pub total_volume_lsat:  u64,
    pub total_accounts:     u64,
    pub total_apps:         u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppState {
    pub version:             u32,
    pub total_transactions:  u64,
    pub balances:            HashMap<String, Lsat>,
    pub app_token_balances:  HashMap<String, u64>,   // key: "app_id:TOKEN:address"
    pub apps:                HashMap<String, AppInfo>,
    pub processed_txs:       HashSet<String>,         // BTC txids already handled
    pub executed_signatures: HashSet<String>,         // replay attack protection
    pub withdrawals:         HashMap<String, WithdrawalRequest>,
    pub history:             Vec<TxRecord>,
    pub last_faucet_claim:   HashMap<String, i64>,
    pub latest_state_root:   String,
    pub stats:               NetworkStats,
}

// ── Constructor ───────────────────────────────────────────────────────────────

impl AppState {
    pub fn new() -> Self {
        Self {
            version:             2,
            total_transactions:  0,
            balances:            HashMap::new(),
            app_token_balances:  HashMap::new(),
            apps:                HashMap::new(),
            processed_txs:       HashSet::new(),
            executed_signatures: HashSet::new(),
            withdrawals:         HashMap::new(),
            history:             Vec::new(),
            last_faucet_claim:   HashMap::new(),
            latest_state_root:   "Genesis".to_string(),
            stats:               NetworkStats::default(),
        }
    }
}

// ── Balance helpers ───────────────────────────────────────────────────────────

impl AppState {
    pub fn get_balance(&self, addr: &str) -> Lsat {
        *self.balances.get(addr).unwrap_or(&0)
    }

    pub fn set_balance(&mut self, addr: &str, amount: Lsat) {
        if amount == 0 {
            self.balances.remove(addr);
        } else {
            self.balances.insert(addr.to_string(), amount);
        }
        self.stats.total_accounts = self.balances.len() as u64;
    }

    pub fn get_app_token_balance(&self, app_id: &str, token: &str, addr: &str) -> u64 {
        let key = format!("{}:{}:{}", app_id, token, addr);
        *self.app_token_balances.get(&key).unwrap_or(&0)
    }

    fn set_app_token_balance(&mut self, app_id: &str, token: &str, addr: &str, amount: u64) {
        let key = format!("{}:{}:{}", app_id, token, addr);
        if amount == 0 {
            self.app_token_balances.remove(&key);
        } else {
            self.app_token_balances.insert(key, amount);
        }
    }
}

// ── History helpers ───────────────────────────────────────────────────────────

impl AppState {
    pub fn add_record_full(
        &mut self,
        tx_type: &str, token: &str, from: &str, to: &str,
        amount: u64, txid: &str,
        memo: Option<String>, btc_txid: Option<String>, status: &str,
    ) -> TxRecord {
        let record = TxRecord {
            tx_type:   tx_type.to_string(),
            token:     token.to_string(),
            from:      from.to_string(),
            to:        to.to_string(),
            amount,
            txid:      txid.to_string(),
            timestamp: Utc::now().timestamp(),
            memo,
            btc_txid,
            status:    status.to_string(),
        };
        self.history.push(record.clone());
        // Rolling window — keep last MAX_HISTORY records
        if self.history.len() > MAX_HISTORY {
            self.history.drain(0..self.history.len() - MAX_HISTORY);
        }
        record
    }
}

// ── Unique txid helper ────────────────────────────────────────────────────────

fn unique_txid(prefix: &str, a: &str, b: &str, n: u64) -> String {
    let mut h = Sha256::new();
    h.update(a.as_bytes());
    h.update(b.as_bytes());
    h.update(n.to_le_bytes());
    h.update(Utc::now().timestamp_nanos_opt().unwrap_or(0).to_le_bytes());
    let hash = h.finalize();
    let short = u32::from_le_bytes(hash[..4].try_into().unwrap());
    format!("{}_{}_{:08x}", prefix, Utc::now().timestamp_millis(), short)
}

// ── Transaction processors ────────────────────────────────────────────────────

impl AppState {
    /// Testnet faucet — 10,000 LSAT, once per 24h per address
    pub fn process_faucet(&mut self, address: &str) -> Result<TxRecord, String> {
        if address.is_empty() {
            return Err("Address required".into());
        }
        let now = Utc::now().timestamp();
        let last = *self.last_faucet_claim.get(address).unwrap_or(&0);
        if now - last < FAUCET_COOLDOWN_SEC {
            let wait = FAUCET_COOLDOWN_SEC - (now - last);
            let hours = wait / 3600;
            let mins  = (wait % 3600) / 60;
            return Err(format!("Faucet cooldown: {}h {}m remaining", hours, mins));
        }

        let current = self.get_balance(address);
        self.set_balance(address, current + FAUCET_AMOUNT);
        self.last_faucet_claim.insert(address.to_string(), now);
        self.total_transactions += 1;

        Ok(self.add_record_full(
            "Faucet", "LSAT", "System", address,
            FAUCET_AMOUNT, "L2_FAUCET",
            Some("Testnet faucet — 10,000 LSAT".into()), None, "Confirmed",
        ))
    }

    /// Transfer LSAT between addresses
    pub fn process_transfer(
        &mut self, sender: &str, recipient: &str,
        amount: Lsat, memo: Option<String>,
    ) -> Result<TxRecord, String> {
        if amount == 0 {
            return Err("Amount must be > 0".into());
        }
        if sender.is_empty() || recipient.is_empty() {
            return Err("Sender and recipient required".into());
        }
        if sender == recipient {
            return Err("Cannot transfer to yourself".into());
        }
        let sender_bal = self.get_balance(sender);
        if sender_bal < amount {
            return Err(format!("Insufficient LSAT: have {}, need {}", sender_bal, amount));
        }

        self.set_balance(sender, sender_bal - amount);
        let rec_bal = self.get_balance(recipient);
        self.set_balance(recipient, rec_bal + amount);

        self.total_transactions      += 1;
        self.stats.total_transfers   += 1;
        self.stats.total_volume_lsat += amount;

        let txid = unique_txid("L2", sender, recipient, amount);
        Ok(self.add_record_full(
            "Transfer", "LSAT", sender, recipient,
            amount, &txid, memo, None, "Confirmed",
        ))
    }

    /// PegIn: BTC confirmed on L1 → mint LSAT
    pub fn process_pegin(&mut self, to_addr: &str, amount: Lsat, btc_txid: &str) -> TxRecord {
        let current = self.get_balance(to_addr);
        self.set_balance(to_addr, current + amount);
        self.processed_txs.insert(btc_txid.to_string());
        self.total_transactions    += 1;
        self.stats.total_pegins    += 1;

        let txid = format!("PEGIN_{}", btc_txid);
        self.add_record_full(
            "PegIn", "LSAT", "Bitcoin L1", to_addr,
            amount, &txid,
            Some("BTC → LSAT (1:1 peg)".into()),
            Some(btc_txid.to_string()), "Confirmed",
        )
    }

    /// PegOut: lock LSAT, start 24h challenge window, then send BTC
    pub fn queue_withdrawal(
        &mut self, user: String, btc_address: String,
        amount: Lsat, _operator_addr: String,
    ) -> Result<(String, TxRecord), String> {
        if amount < DUST_LIMIT {
            return Err(format!(
                "Amount {} LSAT below Bitcoin dust limit ({})",
                amount, DUST_LIMIT
            ));
        }
        if btc_address.is_empty() {
            return Err("Bitcoin address required".into());
        }
        let user_bal = self.get_balance(&user);
        if user_bal < amount {
            return Err(format!("Insufficient LSAT: have {}, need {}", user_bal, amount));
        }

        // Lock LSAT immediately
        self.set_balance(&user, user_bal - amount);

        let now    = Utc::now().timestamp();
        let req_id = unique_txid("WD", &user, &btc_address, amount);
        let req    = WithdrawalRequest {
            id:                 req_id.clone(),
            user:               user.clone(),
            btc_address:        btc_address.clone(),
            amount,
            status:             WithdrawalStatus::Pending,
            created_at:         now,
            challenge_deadline: now + CHALLENGE_WINDOW_SEC,
            retry_count:        0,
        };
        self.withdrawals.insert(req_id.clone(), req);
        self.total_transactions   += 1;
        self.stats.total_pegouts  += 1;

        let record = self.add_record_full(
            "PegOut", "LSAT", &user, &btc_address,
            amount, &req_id,
            Some("LSAT → BTC | 24h challenge window".into()),
            None, "InChallenge",
        );
        Ok((req_id, record))
    }

    // ── App Tokens ────────────────────────────────────────────────────────────

    /// Register a new app and its token economy
    pub fn process_app_register(
        &mut self, owner: &str, app_id: &str, app_name: &str,
        token_name: &str, rate_per_lsat: u64,
        description: &str, website: Option<String>,
    ) -> Result<TxRecord, String> {
        if app_id.is_empty()        { return Err("App ID required".into()); }
        if token_name.is_empty()    { return Err("Token name required".into()); }
        if rate_per_lsat == 0       { return Err("Rate must be > 0".into()); }
        if self.apps.contains_key(app_id) {
            return Err(format!("App '{}' already registered", app_id));
        }

        self.apps.insert(app_id.to_string(), AppInfo {
            app_id:        app_id.to_string(),
            app_name:      app_name.to_string(),
            owner:         owner.to_string(),
            description:   description.to_string(),
            website,
            token_name:    token_name.to_string(),
            rate_per_lsat,
            created_at:    Utc::now().timestamp(),
            lsat_collected: 0,
        });

        self.total_transactions += 1;
        self.stats.total_apps    = self.apps.len() as u64;

        let txid = format!("APP_{}", app_id);
        Ok(self.add_record_full(
            "AppRegister", "LSAT", owner, app_id, 0, &txid,
            Some(format!(
                "App '{}' registered | 1 LSAT = {} {}",
                app_name, rate_per_lsat, token_name
            )),
            None, "Confirmed",
        ))
    }

    /// Buy app tokens with LSAT
    pub fn process_buy_app_token(
        &mut self, buyer: &str, app_id: &str, lsat_amount: Lsat,
    ) -> Result<TxRecord, String> {
        let app = self.apps.get(app_id)
            .cloned()
            .ok_or_else(|| format!("App '{}' not found", app_id))?;

        if lsat_amount == 0 {
            return Err("Amount must be > 0".into());
        }
        let buyer_bal = self.get_balance(buyer);
        if buyer_bal < lsat_amount {
            return Err(format!(
                "Insufficient LSAT: have {}, need {}", buyer_bal, lsat_amount
            ));
        }

        let tokens_received = lsat_amount
            .checked_mul(app.rate_per_lsat)
            .ok_or("Token overflow")?;

        // Owner buying own tokens: no LSAT movement (minting for self/testing)
        // Regular buyer: LSAT flows to app owner
        if buyer != app.owner {
            self.set_balance(buyer, buyer_bal - lsat_amount);
            let owner_bal = self.get_balance(&app.owner);
            self.set_balance(&app.owner, owner_bal + lsat_amount);
        }

        // Mint tokens to buyer
        let cur = self.get_app_token_balance(app_id, &app.token_name, buyer);
        self.set_app_token_balance(app_id, &app.token_name, buyer, cur + tokens_received);

        if let Some(a) = self.apps.get_mut(app_id) {
            a.lsat_collected += lsat_amount;
        }

        self.total_transactions      += 1;
        self.stats.total_volume_lsat += lsat_amount;

        let txid = unique_txid("TOKEN", buyer, app_id, lsat_amount);
        Ok(self.add_record_full(
            "AppTokenBuy", &app.token_name, buyer, app_id,
            tokens_received, &txid,
            Some(format!(
                "Spent {} LSAT → {} {}",
                lsat_amount, tokens_received, app.token_name
            )),
            None, "Confirmed",
        ))
    }

    /// Transfer app tokens between addresses
    pub fn process_app_token_transfer(
        &mut self, app_id: &str, from: &str, to: &str, amount: u64,
    ) -> Result<TxRecord, String> {
        if amount == 0 { return Err("Amount must be > 0".into()); }
        if from == to  { return Err("Cannot transfer to yourself".into()); }

        let app = self.apps.get(app_id)
            .cloned()
            .ok_or_else(|| format!("App '{}' not found", app_id))?;

        let from_bal = self.get_app_token_balance(app_id, &app.token_name, from);
        if from_bal < amount {
            return Err(format!(
                "Insufficient {}: have {}, need {}",
                app.token_name, from_bal, amount
            ));
        }

        self.set_app_token_balance(app_id, &app.token_name, from, from_bal - amount);
        let to_bal = self.get_app_token_balance(app_id, &app.token_name, to);
        self.set_app_token_balance(app_id, &app.token_name, to, to_bal + amount);

        self.total_transactions += 1;
        let txid = unique_txid("TOKENTX", from, to, amount);
        Ok(self.add_record_full(
            "AppTokenTransfer", &app.token_name, from, to,
            amount, &txid,
            Some(format!("{} {} in app {}", amount, app.token_name, app_id)),
            None, "Confirmed",
        ))
    }

    // ── API serialization ─────────────────────────────────────────────────────

    pub fn to_api_json(&self, operator: &str) -> serde_json::Value {
        use serde_json::json;

        let apps_list: Vec<serde_json::Value> = self.apps.values().map(|a| json!({
            "app_id":        a.app_id,
            "app_name":      a.app_name,
            "owner":         a.owner,
            "token_name":    a.token_name,
            "rate_per_lsat": a.rate_per_lsat,
            "description":   a.description,
            "website":       a.website,
            "lsat_collected": a.lsat_collected,
            "created_at":    a.created_at,
        })).collect();

        let pending_withdrawals = self.withdrawals.values()
            .filter(|w| w.status == WithdrawalStatus::Pending)
            .count();

        json!({
            "version":             "2.0",
            "network":             "Lumen-Testnet",
            "token": {
                "name":   "Lumen Satoshi",
                "symbol": "LSAT",
                "peg":    "1 LSAT = 1 Bitcoin Satoshi"
            },
            "total_transactions":  self.total_transactions,
            "latest_state_root":   self.latest_state_root,
            "operator":            operator,
            "balances":            self.balances,
            "history":             self.history,
            "apps":                apps_list,
            "stats":               self.stats,
            "pending_withdrawals": pending_withdrawals,
        })
    }
}