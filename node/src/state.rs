use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use chrono::Utc;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use crate::vm;

pub const DUST_LIMIT: u64 = 546;
pub const TX_FEE: u64 = 100;

// Helper function to decode hex string into bytes
fn hex_to_bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0))
        .collect()
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TokenMetadata {
    pub ticker: String,
    pub name: String,
    pub supply: u64,
    pub issuer: String,
    pub description: String,
    pub created_at: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum WithdrawalStatus {
    Pending,
    Completed(String), 
    Failed(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WithdrawalRequest {
    pub id: String,
    pub user: String,
    pub amount: u64,
    pub status: WithdrawalStatus,
    pub created_at: i64,
    pub retry_count: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TxRecord {
    pub tx_type: String,
    pub token: String,
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub txid: String,
    pub timestamp: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppState {
    pub total_transactions: u64,
    pub balances: HashMap<String, HashMap<String, u64>>,
    pub tokens: HashMap<String, TokenMetadata>,
    pub processed_txs: HashSet<String>,
    pub executed_signatures: HashSet<String>, 
    pub withdrawals: HashMap<String, WithdrawalRequest>, 
    pub history: Vec<TxRecord>,
    pub last_faucet_claim: HashMap<String, i64>,
    pub latest_state_root: String, 
    // NEW: On-chain program storage (Smart Contracts)
    pub programs: HashMap<String, Vec<u8>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            total_transactions: 0,
            balances: HashMap::new(),
            tokens: HashMap::new(),
            processed_txs: HashSet::new(),
            executed_signatures: HashSet::new(),
            withdrawals: HashMap::new(),
            history: Vec::new(),
            last_faucet_claim: HashMap::new(),
            latest_state_root: String::from("Unanchored"), 
            programs: HashMap::new(),
        }
    }

    pub fn add_record(&mut self, tx_type: &str, token: &str, from: &str, to: &str, amount: u64, txid: &str) -> TxRecord {
        let record = TxRecord {
            tx_type: tx_type.to_string(),
            token: token.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            amount,
            txid: txid.to_string(),
            timestamp: Utc::now().timestamp(),
        };
        self.history.push(record.clone());
        if self.history.len() > 1000 { self.history.remove(0); }
        record
    }

    pub fn get_balance(&self, user: &str, token: &str) -> u64 {
        self.balances.get(user).and_then(|t| t.get(token)).cloned().unwrap_or(0)
    }

    pub fn set_balance(&mut self, user: &str, token: &str, amount: u64) {
        let user_bals = self.balances.entry(user.to_string()).or_insert_with(HashMap::new);
        if amount == 0 { user_bals.remove(token); } 
        else { user_bals.insert(token.to_string(), amount); }
    }

    pub fn process_faucet(&mut self, sender: &str) -> Result<TxRecord, String> {
        let now = Utc::now().timestamp();
        let last = *self.last_faucet_claim.get(sender).unwrap_or(&0);
        if now - last < 10 { return Err("Cooldown active".into()); }

        let current = self.get_balance(sender, "BTC");
        self.set_balance(sender, "BTC", current + 20000);
        self.last_faucet_claim.insert(sender.to_string(), now);
        self.total_transactions += 1;
        Ok(self.add_record("Faucet", "BTC", "System", sender, 20000, "L2_FREE"))
    }

    pub fn process_deploy(&mut self, sender: &str, ticker: String, name: String, supply: u64, desc: String) -> Result<TxRecord, String> {
        if self.tokens.contains_key(&ticker) { return Err("Token exists".into()); }
        if ticker == "BTC" { return Err("BTC reserved".into()); }

        let meta = TokenMetadata {
            ticker: ticker.clone(), name, supply, issuer: sender.to_string(), description: desc, created_at: Utc::now().timestamp()
        };
        self.tokens.insert(ticker.clone(), meta);
        self.set_balance(sender, &ticker, supply);
        self.total_transactions += 1;
        Ok(self.add_record("Deploy", &ticker, "System", sender, supply, "Genesis"))
    }

    // NEW: Smart Contract Deployment Logic
    pub fn process_deploy_program(&mut self, sender: &str, program_id: &str, bytecode_hex: &str) -> Result<TxRecord, String> {
        if self.programs.contains_key(program_id) {
            return Err("Program ID already exists".into());
        }
        let bytecode = hex_to_bytes(bytecode_hex);
        if bytecode.is_empty() {
            return Err("Invalid or empty bytecode".into());
        }

        self.programs.insert(program_id.to_string(), bytecode);
        self.total_transactions += 1;
        
        Ok(self.add_record("ContractDeploy", "CODE", sender, program_id, 0, "L2_DEPLOY"))
    }

    pub fn process_transfer(&mut self, sender: &str, recipient: &str, ticker: &str, amount: u64, operator: &str) -> Result<TxRecord, String> {
        let sender_bal = self.get_balance(sender, ticker);
        let btc_bal = self.get_balance(sender, "BTC");
        let fee = if ticker == "BTC" { 0 } else { TX_FEE };

        if ticker == "BTC" {
            if sender_bal < amount + TX_FEE { return Err("Insufficient BTC".into()); }
        } else {
            if sender_bal < amount || btc_bal < fee { return Err("Insufficient funds/fees".into()); }
        }

        self.set_balance(sender, ticker, sender_bal - amount);
        let rec_bal = self.get_balance(recipient, ticker);
        self.set_balance(recipient, ticker, rec_bal + amount);

        if fee > 0 {
            self.set_balance(sender, "BTC", btc_bal - fee);
            let op_bal = self.get_balance(operator, "BTC");
            self.set_balance(operator, "BTC", op_bal + fee);
        }

        self.total_transactions += 1;
        Ok(self.add_record("Transfer", ticker, sender, recipient, amount, "L2_TX"))
    }

    // MODIFIED: Execute now reads bytecode from state
    pub fn process_execute(&mut self, sender: &str, sender_pk_hex: &str, program_id: &str) -> Result<TxRecord, String> {
        let prog_pubkey = Pubkey::from_str(program_id).map_err(|_| "Invalid Program ID")?;
        
        let bytecode = self.programs.get(program_id).cloned().ok_or("Program not found. Deploy it first!")?;
        
        let gas_limit = 10000;
        let mut seed = [0u8; 32];
        let bytes = sender_pk_hex.as_bytes();
        let len = std::cmp::min(bytes.len(), 32);
        seed[..len].copy_from_slice(&bytes[..len]);
        let vm_key = Pubkey::new_from_array(seed);

        let mut accounts = HashMap::new();
        accounts.insert(vm_key, solana_sdk::account::Account {
            lamports: self.get_balance(sender, "BTC"),
            data: vec![],
            owner: prog_pubkey,
            executable: false,
            rent_epoch: 0,
        });

        // Pass the dynamically fetched bytecode to the VM
        let result = vm::LumenVM::execute(&prog_pubkey, &mut accounts, &bytecode, gas_limit).map_err(|e| e.to_string())?;

        for log in &result.logs {
            println!("  [SVM LOG]: {}", log);
        }

        if let Some(acc) = result.new_accounts.get(&vm_key) {
            self.set_balance(sender, "BTC", acc.lamports);
        }

        self.total_transactions += 1;
        Ok(self.add_record("VM_Execute", "BTC", "System", sender, 0, "SVM_SUCCESS"))
    }

    pub fn queue_withdrawal(&mut self, user: String, amount: u64, fee: u64, operator_addr: String) -> Result<(String, TxRecord), String> {
        if amount < DUST_LIMIT {
            return Err(format!("Below dust limit: {}", amount));
        }

        let user_bal = self.get_balance(&user, "BTC");
        let total_needed = amount + fee;

        if user_bal < total_needed {
            return Err("Insufficient BTC for withdrawal".into());
        }

        self.set_balance(&user, "BTC", user_bal - total_needed);
        let op_bal = self.get_balance(&operator_addr, "BTC");
        self.set_balance(&operator_addr, "BTC", op_bal + fee);

        let req_id = format!("{}_{}", user, Utc::now().timestamp_millis());
        let req = WithdrawalRequest {
            id: req_id.clone(),
            user: user.clone(),
            amount,
            status: WithdrawalStatus::Pending,
            created_at: Utc::now().timestamp(),
            retry_count: 0,
        };

        self.withdrawals.insert(req_id.clone(), req);
        self.total_transactions += 1;
        
        let record = self.add_record("WithdrawRequest", "BTC", &user, "Pending", amount, &req_id);
        Ok((req_id, record))
    }
}