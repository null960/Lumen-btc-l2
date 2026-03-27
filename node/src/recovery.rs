// ── Lumen Network — State Recovery ──────────────────────────────────────────
//
// Recovery priority:
//   1. redb database        — fastest, normal startup
//   2. Latest DA snapshot   — full state at a point in time
//   3. Replay all DA batches — transaction by transaction from genesis
//   4. Pending batch         — txs since last full batch (crash protection)
//
// After recovery the state root is recomputed and verified.

use crate::state::{AppState, AppInfo, TxRecord, WithdrawalStatus, WithdrawalRequest};
use crate::da_adapter::BitcoinDAAdapter;
use crate::settlement::build_merkle_root;
use chrono::Utc;

// ── Report ────────────────────────────────────────────────────────────────────

#[derive(Debug)]
#[allow(dead_code)]
pub struct RecoveryReport {
    pub batches_replayed:      usize,
    pub transactions_replayed: usize,
    pub final_state_root:      String,
    pub accounts_recovered:    usize,
    pub apps_recovered:        usize,
    pub warnings:              Vec<String>,
}

// ── Main recovery entry point ─────────────────────────────────────────────────

pub fn recover_from_da(
    da: &BitcoinDAAdapter,
    expected_root: Option<&str>,
) -> (AppState, RecoveryReport) {
    println!("🔄 Starting state recovery from DA layer...");

    let mut warnings  = Vec::new();
    let batches       = da.load_all_batches();
    println!("   Found {} DA batches", batches.len());

    // ── Try snapshot first (fast path) ────────────────────────────────────────
    let (mut state, start_from) = match da.load_latest_snapshot() {
        Some(snapshot) => {
            println!(
                "   📸 Snapshot loaded — {} txs, {} accounts",
                snapshot.total_transactions,
                snapshot.balances.len()
            );
            // Find which batch index comes after snapshot
            let snap_txs = snapshot.total_transactions;
            let mut cumulative = 0u64;
            let mut start = 0usize;
            for (i, batch) in batches.iter().enumerate() {
                cumulative += batch.transactions.len() as u64;
                if cumulative >= snap_txs {
                    start = i + 1;
                    break;
                }
            }
            println!("   Replaying {} batches after snapshot", batches.len().saturating_sub(start));
            (snapshot, start)
        }
        None => {
            println!("   No snapshot — replaying all batches from genesis");
            (AppState::new(), 0)
        }
    };

    // ── Replay batches ────────────────────────────────────────────────────────
    let batches_to_replay = &batches[start_from..];
    let mut txs_replayed  = 0usize;

    for batch in batches_to_replay {
        println!(
            "   Batch #{}: {} txs ({})",
            batch.id,
            batch.transactions.len(),
            chrono::DateTime::from_timestamp(batch.timestamp, 0)
                .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_default()
        );

        for tx in &batch.transactions {
            // Skip if already in history (snapshot overlap)
            if state.history.iter().any(|r| r.txid == tx.txid) {
                continue;
            }

            match replay_tx(&mut state, tx) {
                Ok(_) => {
                    txs_replayed += 1;
                    state.history.push(tx.clone());
                }
                Err(e) => {
                    let w = format!(
                        "Batch #{} tx '{}' skipped: {}",
                        batch.id,
                        &tx.txid[..tx.txid.len().min(16)],
                        e
                    );
                    println!("   ⚠️  {}", w);
                    warnings.push(w);
                }
            }
        }

        // Verify batch state root if present
        if !batch.state_root.is_empty() {
            let computed = build_merkle_root(&state.balances);
            if computed != batch.state_root {
                let w = format!(
                    "Batch #{} root mismatch (expected {}... got {}...)",
                    batch.id,
                    &batch.state_root[..batch.state_root.len().min(12)],
                    &computed[..computed.len().min(12)]
                );
                println!("   ⚠️  {}", w);
                warnings.push(w);
            }
        }
    }

    // ── Replay pending batch (crash protection) ───────────────────────────────
    if let Some(pending) = da.load_pending_batch() {
        if !pending.transactions.is_empty() {
            println!("   Pending batch: {} txs", pending.transactions.len());
            for tx in &pending.transactions {
                if state.history.iter().any(|r| r.txid == tx.txid) {
                    continue;
                }
                if let Ok(()) = replay_tx(&mut state, tx) {
                    txs_replayed += 1;
                    state.history.push(tx.clone());
                }
            }
        }
    }

    // ── Recompute counters ────────────────────────────────────────────────────
    state.total_transactions = state.history.len() as u64;

    // ── Final root verification ───────────────────────────────────────────────
    let final_root = build_merkle_root(&state.balances);
    state.latest_state_root = final_root.clone();

    if let Some(expected) = expected_root {
        if final_root != expected {
            let w = format!(
                "Final root mismatch vs L1 (got {}... expected {}...)",
                &final_root[..final_root.len().min(12)],
                &expected[..expected.len().min(12)]
            );
            println!("   ⚠️  {}", w);
            warnings.push(w);
        } else {
            println!("   ✅ State root matches Bitcoin L1");
        }
    }

    let report = RecoveryReport {
        batches_replayed:      batches_to_replay.len(),
        transactions_replayed: txs_replayed,
        final_state_root:      final_root.clone(),
        accounts_recovered:    state.balances.len(),
        apps_recovered:        state.apps.len(),
        warnings,
    };

    println!("✅ Recovery complete:");
    println!("   Batches   : {}", report.batches_replayed);
    println!("   Txs       : {}", report.transactions_replayed);
    println!("   Accounts  : {}", report.accounts_recovered);
    println!("   Apps      : {}", report.apps_recovered);
    println!("   Root      : {}...", &final_root[..final_root.len().min(16)]);
    if !report.warnings.is_empty() {
        println!("   Warnings  : {}", report.warnings.len());
    }

    (state, report)
}

// ── Transaction replay ────────────────────────────────────────────────────────

fn replay_tx(state: &mut AppState, tx: &TxRecord) -> Result<(), String> {
    match tx.tx_type.as_str() {

        // ── LSAT transfers ────────────────────────────────────────────────────
        "Transfer" => {
            if tx.amount > 0
                && !tx.from.is_empty() && !tx.to.is_empty()
                && tx.from != "System"
                && tx.from != "Bitcoin L1"
            {
                let from_bal = state.get_balance(&tx.from);
                if from_bal >= tx.amount {
                    state.set_balance(&tx.from, from_bal - tx.amount);
                    let to_bal = state.get_balance(&tx.to);
                    state.set_balance(&tx.to, to_bal + tx.amount);
                    state.stats.total_transfers   += 1;
                    state.stats.total_volume_lsat += tx.amount;
                }
            }
            Ok(())
        }

        "Faucet" | "PegIn" => {
            if tx.amount > 0 && !tx.to.is_empty() {
                let bal = state.get_balance(&tx.to);
                state.set_balance(&tx.to, bal + tx.amount);
                if tx.tx_type == "PegIn" { state.stats.total_pegins += 1; }
                if let Some(btxid) = &tx.btc_txid {
                    state.processed_txs.insert(btxid.clone());
                }
            }
            Ok(())
        }

        "PegOut" => {
            // LSAT was already deducted at queue time — just restore the
            // withdrawal request so the 24h window check still works
            if tx.status == "InChallenge" || tx.status == "Pending" {
                if !state.withdrawals.contains_key(&tx.txid) {
                    state.withdrawals.insert(tx.txid.clone(), WithdrawalRequest {
                        id:                 tx.txid.clone(),
                        user:               tx.from.clone(),
                        btc_address:        tx.to.clone(),
                        amount:             tx.amount,
                        status:             WithdrawalStatus::Pending,
                        created_at:         tx.timestamp,
                        challenge_deadline: tx.timestamp + crate::state::CHALLENGE_WINDOW_SEC,
                        retry_count:        0,
                    });
                    state.stats.total_pegouts += 1;
                }
            }
            Ok(())
        }

        // ── App Tokens ────────────────────────────────────────────────────────
        "AppRegister" => {
            // tx.from = owner, tx.to = app_id
            // memo format: "App 'NAME' registered | 1 LSAT = N TOKEN"
            if !state.apps.contains_key(&tx.to) {
                let (token_name, rate) = parse_app_memo(tx.memo.as_deref());
                state.apps.insert(tx.to.clone(), AppInfo {
                    app_id:         tx.to.clone(),
                    app_name:       tx.to.clone(), // best effort without dedicated field
                    owner:          tx.from.clone(),
                    description:    String::new(),
                    website:        None,
                    token_name,
                    rate_per_lsat:  rate,
                    created_at:     tx.timestamp,
                    lsat_collected: 0,
                });
                state.stats.total_apps = state.apps.len() as u64;
            }
            Ok(())
        }

        "AppTokenBuy" => {
            // tx.from = buyer, tx.to = app_id, tx.token = token name, tx.amount = tokens received
            // memo format: "Spent N LSAT → M TOKEN"
            if tx.amount > 0 && !tx.to.is_empty() {
                let key = format!("{}:{}:{}", tx.to, tx.token, tx.from);
                let cur = *state.app_token_balances.get(&key).unwrap_or(&0);
                state.app_token_balances.insert(key, cur + tx.amount);

                // Restore LSAT collected and owner balance
                let lsat_spent = parse_lsat_spent(tx.memo.as_deref());
                if lsat_spent > 0 {
                    // Extract what we need BEFORE any mutable borrow
                    let (owner, is_owner_buying) = if let Some(app) = state.apps.get(&tx.to) {
                        (app.owner.clone(), app.owner == tx.from)
                    } else {
                        (String::new(), true)
                    };

                    // Now do mutable operations separately
                    if let Some(app) = state.apps.get_mut(&tx.to) {
                        app.lsat_collected += lsat_spent;
                    }

                    if !is_owner_buying && !owner.is_empty() {
                        let from_bal = state.get_balance(&tx.from);
                        if from_bal >= lsat_spent {
                            state.set_balance(&tx.from, from_bal - lsat_spent);
                        }
                        let owner_bal = state.get_balance(&owner);
                        state.set_balance(&owner, owner_bal + lsat_spent);
                    }

                    state.stats.total_volume_lsat += lsat_spent;
                }
            }
            Ok(())
        }

        "AppTokenTransfer" => {
            // tx.from = sender, tx.to = recipient, tx.token = token, tx.amount = amount
            if tx.amount > 0 && !tx.to.is_empty() {
                // Find which app this token belongs to
                let app_id = state.apps.iter()
                    .find(|(_, a)| a.token_name == tx.token)
                    .map(|(id, _)| id.clone());

                if let Some(app_id) = app_id {
                    let from_key = format!("{}:{}:{}", app_id, tx.token, tx.from);
                    let to_key   = format!("{}:{}:{}", app_id, tx.token, tx.to);
                    let from_bal = *state.app_token_balances.get(&from_key).unwrap_or(&0);
                    if from_bal >= tx.amount {
                        state.app_token_balances.insert(from_key, from_bal - tx.amount);
                        let to_bal = *state.app_token_balances.get(&to_key).unwrap_or(&0);
                        state.app_token_balances.insert(to_key, to_bal + tx.amount);
                    }
                }
            }
            Ok(())
        }

        // Settlement records — just restore state root
        "Settlement" => {
            if let Some(memo) = &tx.memo {
                if memo.starts_with("Root: ") {
                    let root_prefix = memo.trim_start_matches("Root: ")
                        .trim_end_matches("...");
                    if !root_prefix.is_empty() {
                        state.latest_state_root = root_prefix.to_string();
                    }
                }
            }
            Ok(())
        }

        // Unknown types — skip safely, log
        other => {
            Err(format!("Unknown tx type: {}", other))
        }
    }
}

// ── Memo parsers ──────────────────────────────────────────────────────────────

/// Parse "App 'NAME' registered | 1 LSAT = 10 GOLD" → ("GOLD", 10)
fn parse_app_memo(memo: Option<&str>) -> (String, u64) {
    let memo = match memo { Some(m) => m, None => return ("TOKEN".into(), 1) };
    // "... | 1 LSAT = 10 GOLD"
    if let Some(rate_part) = memo.split('|').nth(1) {
        let words: Vec<&str> = rate_part.split_whitespace().collect();
        // ["1", "LSAT", "=", "10", "GOLD"]
        let rate  = words.get(3).and_then(|s| s.parse::<u64>().ok()).unwrap_or(1);
        let token = words.get(4).map(|s| s.to_string()).unwrap_or_else(|| "TOKEN".into());
        return (token, rate);
    }
    ("TOKEN".into(), 1)
}

/// Parse "Spent 100 LSAT → 1000 GOLD" → 100
fn parse_lsat_spent(memo: Option<&str>) -> u64 {
    let memo = match memo { Some(m) => m, None => return 0 };
    // "Spent N LSAT ..."
    let words: Vec<&str> = memo.split_whitespace().collect();
    if words.first().map(|s| *s) == Some("Spent") {
        return words.get(1).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    }
    0
}

// ── Balance snapshot export ───────────────────────────────────────────────────

pub fn export_balance_snapshot(state: &AppState) -> String {
    let mut lines = vec![
        "# Lumen Network — Balance Snapshot".to_string(),
        format!("# Generated:   {}", Utc::now().to_rfc3339()),
        format!("# State Root:  {}", state.latest_state_root),
        format!("# Total Txs:   {}", state.total_transactions),
        format!("# Accounts:    {}", state.balances.len()),
        format!("# Apps:        {}", state.apps.len()),
        String::new(),
        "## LSAT Balances".to_string(),
    ];

    let mut sorted: Vec<_> = state.balances.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1)); // descending by balance
    for (addr, bal) in &sorted {
        lines.push(format!("{}: {} LSAT", addr, bal));
    }

    if !state.apps.is_empty() {
        lines.push(String::new());
        lines.push("## Registered Apps".to_string());
        for app in state.apps.values() {
            lines.push(format!(
                "{} | {} | 1 LSAT = {} {} | owner: {}",
                app.app_id, app.app_name,
                app.rate_per_lsat, app.token_name,
                app.owner
            ));
        }
    }

    if !state.withdrawals.is_empty() {
        lines.push(String::new());
        lines.push("## Pending Withdrawals".to_string());
        for wd in state.withdrawals.values() {
            if wd.status == crate::state::WithdrawalStatus::Pending {
                lines.push(format!(
                    "{}: {} LSAT → {} (deadline: {})",
                    wd.id, wd.amount, wd.btc_address, wd.challenge_deadline
                ));
            }
        }
    }

    lines.join("\n")
}