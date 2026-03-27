use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use chrono::Utc;
use bitcoin::consensus::encode::serialize_hex;

use crate::state::AppState;

mod da_adapter;
mod rpc;
mod state;
mod storage;
mod wallet;
mod btc_api;
mod withdraw;
mod settlement;
mod recovery;
mod rate_limit;
mod webhook;

const SETTLEMENT_INTERVAL:    u64 = 180;
const SYNC_INTERVAL_SEC:      u64 = 30;
const SNAPSHOT_EVERY_N_BATCHES: u64 = 20;

// ── Network event types ───────────────────────────────────────────────────────

#[derive(Debug)]
pub enum NetworkEvent {
    /// Signed user transaction — signature verified before execution
    Transaction(String),
    /// Internal system command — no signature needed (faucet, CLI transfer)
    SystemCommand { address: String, cmd: String },
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    // ── Load operator wallet ─────────────────────────────────────────────────
    let wallet = if let Ok(wif) = std::env::var("OPERATOR_WIF") {
        wallet::LocalWallet::from_wif(&wif)
    } else if std::path::Path::new(".env").exists() {
        eprintln!("❌ FATAL: .env found but OPERATOR_WIF is not set!");
        eprintln!("   Edit .env and set: OPERATOR_WIF=your_wif_key");
        std::process::exit(1);
    } else {
        // First run — generate key and write .env
        println!("🔑 First run — generating operator key...");
        let w = wallet::LocalWallet::new_random();
        let env = format!(
            "# Lumen Network — auto-generated\n\
             # NEVER commit this file to git!\n\
             OPERATOR_WIF={}\n\
             LUMEN_NETWORK=testnet\n\
             LUMEN_PORT=3000\n",
            w.secret_wif
        );
        std::fs::write(".env", env).expect("Failed to write .env");
        println!("✅ .env created with new operator key");
        println!("👤 Operator address: {}", w.address);
        println!("⚠️  Back up .env — it contains your private key!");
        w
    };

    let operator_address = wallet.address.clone();

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  🟠 Lumen Network Node v2.0");
    println!("  💡 1 LSAT = 1 Bitcoin Satoshi | Zero fees");
    println!("  👤 Operator: {}", operator_address);
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let api_client = btc_api::BtcApi::new();
    let storage    = Arc::new(storage::Storage::new("lumen_db"));
    let da_adapter = Arc::new(da_adapter::BitcoinDAAdapter::new("lumen_da"));

    // ── State loading (DB → DA recovery → fresh) ─────────────────────────────
    let initial_state = match storage.load_state() {
        Some(s) => {
            println!("✅ State loaded from DB ({} txs, {} accounts)",
                s.total_transactions, s.balances.len());
            s
        }
        None => {
            println!("⚠️  Database not found — attempting DA recovery...");
            let batch_count = da_adapter.batch_count();
            if batch_count > 0 {
                let (recovered, report) = recovery::recover_from_da(&da_adapter, None);
                if report.transactions_replayed > 0 {
                    storage.save_state(&recovered).ok();
                    println!("✅ Recovered {} accounts, {} apps",
                        report.accounts_recovered, report.apps_recovered);
                    recovered
                } else {
                    println!("ℹ️  Empty DA layer — starting fresh");
                    AppState::new()
                }
            } else {
                println!("ℹ️  No DA batches — starting fresh");
                AppState::new()
            }
        }
    };

    let app_state = Arc::new(Mutex::new(initial_state));
    let (tx_sender, mut rx_channel) = mpsc::channel::<NetworkEvent>(10_000);

    // ── RPC server ────────────────────────────────────────────────────────────
    {
        let state    = app_state.clone();
        let tx       = tx_sender.clone();
        let addr     = operator_address.clone();
        let webhooks = Arc::new(webhook::WebhookManager::new());
        tokio::spawn(async move {
            rpc::run_server(state, tx, addr, webhooks).await;
        });
    }

    // ── Bitcoin L1 sync + settlement ─────────────────────────────────────────
    {
        let sync_state   = app_state.clone();
        let sync_wallet  = Arc::new(wallet);
        let sync_api     = api_client.clone();
        let sync_storage = storage.clone();

        tokio::spawn(async move {
            let mut last_settlement = Utc::now().timestamp();

            loop {
                tokio::time::sleep(Duration::from_secs(SYNC_INTERVAL_SEC)).await;

                let utxos = match sync_api.get_utxos(&sync_wallet.address).await {
                    Ok(u)  => u,
                    Err(e) => { println!("⚠️  BTC sync error: {}", e); continue; }
                };

                process_deposits(&sync_state, &utxos, &sync_wallet.address, &sync_storage);
                handle_withdrawals(&sync_state, &sync_wallet, &sync_api, &utxos, &sync_storage).await;

                let now = Utc::now().timestamp();
                if now - last_settlement >= SETTLEMENT_INTERVAL as i64 {
                    run_settlement(&sync_state, &sync_wallet, &sync_api, &utxos, &sync_storage, now).await;
                    last_settlement = now;
                }
            }
        });
    }

    // ── Transaction sequencer ─────────────────────────────────────────────────
    let mut batch_counter    = 1u64;
    let mut current_batch: Vec<state::TxRecord> = Vec::new();

    while let Some(event) = rx_channel.recv().await {

        let (sender_addr, cmd_body) = match event {

            NetworkEvent::Transaction(raw) => {
                let parts: Vec<&str> = raw.split('|').collect();
                if parts.len() < 4 { continue; }
                let cmd_body   = parts[1].to_string();
                let sig_b64    = parts[2];
                let pubkey_hex = parts[3];

                // Replay attack protection
                {
                    let mut s = app_state.lock().unwrap();
                    if s.executed_signatures.contains(sig_b64) {
                        println!("⛔ Replay attack blocked");
                        continue;
                    }
                    s.executed_signatures.insert(sig_b64.to_string());
                }

                // Signature verification
                if !wallet::verify_signature(&cmd_body, sig_b64, pubkey_hex) {
                    println!("⚠️  Invalid signature: {}",
                        &cmd_body[..cmd_body.len().min(40)]);
                    continue;
                }

                let addr = wallet::pubkey_to_address(pubkey_hex).unwrap_or_default();
                (addr, cmd_body)
            }

            NetworkEvent::SystemCommand { address, cmd } => (address, cmd),
        };

        let cmd_parts: Vec<&str> = cmd_body.split_whitespace().collect();
        if cmd_parts.is_empty() { continue; }

        let mut state = app_state.lock().unwrap();

        let result: Result<state::TxRecord, String> = match cmd_parts[0] {

            "Faucet" =>
                state.process_faucet(&sender_addr),

            "Transfer" if cmd_parts.len() >= 3 => {
                let amount = cmd_parts[1].parse::<u64>().unwrap_or(0);
                let to     = cmd_parts[2];
                let memo   = if cmd_parts.len() > 3 { Some(cmd_parts[3..].join(" ")) } else { None };
                state.process_transfer(&sender_addr, to, amount, memo)
            }

            "Withdraw" if cmd_parts.len() >= 3 => {
                let amount  = cmd_parts[1].parse::<u64>().unwrap_or(0);
                let btc_addr = cmd_parts[2].to_string();
                state.queue_withdrawal(sender_addr.clone(), btc_addr, amount, operator_address.clone())
                    .map(|(_, rec)| rec)
            }

            "RegisterApp" if cmd_parts.len() >= 5 => {
                let app_id     = cmd_parts[1];
                let token_name = cmd_parts[2];
                let rate: u64  = cmd_parts[3].parse().unwrap_or(1);
                let app_name   = cmd_parts[4];
                let desc = if cmd_parts.len() > 5 { cmd_parts[5..].join(" ") } else { String::new() };
                state.process_app_register(&sender_addr, app_id, app_name, token_name, rate, &desc, None)
            }

            "BuyToken" if cmd_parts.len() >= 3 => {
                let app_id = cmd_parts[1];
                let amount: u64 = cmd_parts[2].parse().unwrap_or(0);
                state.process_buy_app_token(&sender_addr, app_id, amount)
            }

            "TransferToken" if cmd_parts.len() >= 4 => {
                let app_id = cmd_parts[1];
                let to     = cmd_parts[2];
                let amount: u64 = cmd_parts[3].parse().unwrap_or(0);
                state.process_app_token_transfer(app_id, &sender_addr, to, amount)
            }

            _ => Err(format!(
                "Unknown command: '{}'. Valid: Faucet Transfer Withdraw RegisterApp BuyToken TransferToken",
                cmd_parts[0]
            )),
        };

        match result {
            Ok(record) => {
                println!("✅ {} | {} {} | {}→{}",
                    record.tx_type, record.amount, record.token,
                    &record.from[..record.from.len().min(10)],
                    &record.to[..record.to.len().min(10)]);

                current_batch.push(record);

                // Flush pending batch after every tx (crash safety)
                let root = state.latest_state_root.clone();
                da_adapter.save_pending_batch(batch_counter, &current_batch, &root).ok();

                if current_batch.len() >= 5 {
                    da_adapter.submit_batch_with_root(batch_counter, &current_batch, &root).ok();
                    da_adapter.clear_pending_batch(batch_counter).ok();
                    current_batch.clear();
                    batch_counter += 1;

                    if batch_counter % SNAPSHOT_EVERY_N_BATCHES == 0 {
                        println!("📸 Snapshot saved (batch #{})", batch_counter);
                        da_adapter.save_snapshot(&state).ok();
                    }
                }

                storage.save_state(&state).ok();
            }
            Err(e) => println!("❌ {}", e),
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn process_deposits(
    app_state: &Arc<Mutex<AppState>>,
    utxos:     &[btc_api::Utxo],
    operator:  &str,
    storage:   &Arc<storage::Storage>,
) {
    let mut state   = app_state.lock().unwrap();
    let mut changed = false;

    for utxo in utxos {
        if !utxo.status.confirmed                    { continue; }
        if state.processed_txs.contains(&utxo.txid) { continue; }

        println!("📥 PegIn: {} LSAT ({})", utxo.value,
            &utxo.txid[..utxo.txid.len().min(16)]);
        state.process_pegin(operator, utxo.value, &utxo.txid);
        changed = true;
    }

    if changed { storage.save_state(&state).ok(); }
}

async fn handle_withdrawals(
    app_state: &Arc<Mutex<AppState>>,
    wallet:    &Arc<wallet::LocalWallet>,
    api:       &btc_api::BtcApi,
    utxos:     &[btc_api::Utxo],
    storage:   &Arc<storage::Storage>,
) {
    let now = Utc::now().timestamp();
    let mut targets     = Vec::new();
    let mut pending_ids = Vec::new();

    {
        let state = app_state.lock().unwrap();
        for req in state.withdrawals.values() {
            if req.status == state::WithdrawalStatus::Pending
                && now >= req.challenge_deadline
            {
                targets.push(withdraw::BatchTarget {
                    address: req.btc_address.clone(),
                    amount:  req.amount,
                });
                pending_ids.push(req.id.clone());
                if targets.len() >= 5 { break; }
            }
        }
    }

    if targets.is_empty() { return; }

    println!("📤 Processing {} withdrawal(s)...", targets.len());

    match withdraw::create_batch_withdrawal_tx(wallet, utxos.to_vec(), targets) {
        Ok(tx) => {
            let hex = serialize_hex(&tx);
            match api.broadcast_tx(hex).await {
                Ok(txid) => {
                    println!("💸 Withdrawal broadcast: {}", txid);
                    let mut state = app_state.lock().unwrap();
                    for id in &pending_ids {
                        if let Some(req) = state.withdrawals.get_mut(id) {
                            req.status = state::WithdrawalStatus::Completed(txid.clone());
                        }
                        for rec in &mut state.history {
                            if &rec.txid == id {
                                rec.status   = "Completed".into();
                                rec.btc_txid = Some(txid.clone());
                            }
                        }
                    }
                    storage.save_state(&state).ok();
                }
                Err(e) => println!("❌ Withdrawal broadcast failed: {}", e),
            }
        }
        Err(e) => println!("❌ Withdrawal tx build failed: {}", e),
    }
}

async fn run_settlement(
    app_state: &Arc<Mutex<AppState>>,
    wallet:    &Arc<wallet::LocalWallet>,
    api:       &btc_api::BtcApi,
    utxos:     &[btc_api::Utxo],
    storage:   &Arc<storage::Storage>,
    now:       i64,
) {
    let balances = {
        let s = app_state.lock().unwrap();
        s.balances.clone()
    };

    let state_hash = settlement::build_merkle_root(&balances);
    let local_txid = format!("L2_SETTLE_{}", now);

    // Always record L2 checkpoint
    {
        let mut s = app_state.lock().unwrap();
        s.latest_state_root = state_hash.clone();
        let tx_count = s.total_transactions;
        s.add_record_full(
            "Settlement", "LSAT", "Operator", "L2-Checkpoint",
            tx_count, &local_txid,
            Some(format!("Root: {}...", &state_hash[..state_hash.len().min(16)])),
            None, "Confirmed",
        );
        storage.save_state(&s).ok();
    }
    println!("📋 L2 Checkpoint: {}...", &state_hash[..state_hash.len().min(16)]);

    // Try L1 anchor if we have UTXOs
    if utxos.is_empty() { return; }

    match settlement::create_settlement_tx(wallet, utxos.to_vec(), state_hash) {
        Ok(tx) => {
            match api.broadcast_tx(serialize_hex(&tx)).await {
                Ok(txid) => {
                    println!("⛓️  L1 anchor: {}", txid);
                    let mut s = app_state.lock().unwrap();
                    for rec in s.history.iter_mut().rev() {
                        if rec.txid == local_txid {
                            rec.btc_txid = Some(txid.clone());
                            rec.to       = "Bitcoin L1".into();
                            rec.status   = "Finalized".into();
                            break;
                        }
                    }
                    storage.save_state(&s).ok();
                }
                Err(e) => println!("⚠️  L1 anchor failed: {}", e),
            }
        }
        Err(e) => println!("⚠️  Settlement tx build failed: {}", e),
    }
}