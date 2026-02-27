use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use chrono::Utc;
use bitcoin::consensus::encode::serialize_hex;

use crate::state::AppState;
use crate::da_adapter::DataAvailabilityLayer; 

mod da_adapter;
mod rpc;
mod state;
mod storage;
mod wallet;
mod btc_api;
mod withdraw;
mod settlement;
mod vm; 

const SETTLEMENT_INTERVAL: u64 = 180; 
const SYNC_INTERVAL_SEC: u64 = 5;

#[derive(Debug)]
pub enum NetworkEvent {
    Transaction(String),
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    
    let wallet = wallet::LocalWallet::load_or_generate("operator.json");
    let operator_address = wallet.address.clone();

    println!("--------------------------------------------------");
    println!("🚀 Lumen L2 Node (Smart Contracts Enabled)");
    println!("👤 Operator Address: {}", operator_address);
    println!("--------------------------------------------------");

    let api_client = btc_api::BtcApi::new();
    let storage = Arc::new(storage::Storage::new("lumen_db"));
    let da_adapter = Arc::new(da_adapter::BitcoinDAAdapter::new("lumen_da"));

    let initial_state = storage.load_state().unwrap_or_else(AppState::new);
    let app_state = Arc::new(Mutex::new(initial_state));

    let (tx_sender, mut rx_channel) = mpsc::channel::<NetworkEvent>(10000);

    let rpc_state = app_state.clone();
    let rpc_tx = tx_sender.clone();
    let rpc_addr = operator_address.clone();
    tokio::spawn(async move {
        rpc::run_server(rpc_state, rpc_tx, rpc_addr).await;
    });

    let sync_state = app_state.clone();
    let sync_wallet = Arc::new(wallet);
    let sync_api = api_client.clone();
    let sync_storage = storage.clone();
    
    tokio::spawn(async move {
        let mut last_settlement = Utc::now().timestamp();
        
        loop {
            tokio::time::sleep(Duration::from_secs(SYNC_INTERVAL_SEC)).await;
            
            let utxos = match sync_api.get_utxos(&sync_wallet.address).await {
                Ok(u) => u,
                Err(e) => {
                    println!("⚠️ Sync Error: {}", e);
                    continue;
                }
            };

            process_deposits(&sync_state, &utxos, &sync_wallet.address, &sync_storage);
            handle_withdrawals(&sync_state, &sync_wallet, &sync_api, &utxos, &sync_storage).await;

            let now = Utc::now().timestamp();
            if now - last_settlement >= SETTLEMENT_INTERVAL as i64 {
                let balances = {
                    let state = sync_state.lock().unwrap();
                    let mut btc_bals = std::collections::HashMap::new();
                    for (user, bals) in &state.balances {
                        if let Some(&amt) = bals.get("BTC") {
                            btc_bals.insert(user.clone(), amt);
                        }
                    }
                    btc_bals
                };
                
                let state_hash = settlement::build_merkle_root(&balances);
                
                if let Ok(tx) = settlement::create_settlement_tx(&sync_wallet, utxos.clone(), state_hash.clone()) {
                    let tx_hex = serialize_hex(&tx);
                    if let Ok(txid) = sync_api.broadcast_tx(tx_hex).await {
                        println!("⛓️ Merkle Root Anchored in L1: {}", txid);
                        
                        let mut s = sync_state.lock().unwrap();
                        s.latest_state_root = state_hash; 
                        s.processed_txs.insert(txid.clone());
                        s.add_record("Settlement", "BTC", "Sequencer", "L1", 0, &txid);
                        sync_storage.save_state(&s).ok();
                        
                        last_settlement = now;
                    }
                }
            }
        }
    });

    let mut batch_counter = 1;
    let mut current_batch_txs = Vec::new();

    while let Some(event) = rx_channel.recv().await {
        match event {
            NetworkEvent::Transaction(raw_data) => {
                let parts: Vec<&str> = raw_data.split('|').collect();
                if parts.len() < 4 { continue; }

                let cmd_body = parts[1];
                let sig_b64 = parts[2];
                let pubkey_hex = parts[3];

                {
                    let mut state = app_state.lock().unwrap();
                    if state.executed_signatures.contains(sig_b64) {
                        println!("⛔ REPLAY ATTACK BLOCKED");
                        continue;
                    }
                    state.executed_signatures.insert(sig_b64.to_string());
                }

                if !wallet::verify_signature(cmd_body, sig_b64, pubkey_hex) {
                    println!("⚠️ Invalid Signature");
                    continue;
                }

                let sender_addr = wallet::pubkey_to_address(pubkey_hex).unwrap_or_default();
                let cmd_parts: Vec<&str> = cmd_body.split_whitespace().collect();
                
                let mut state = app_state.lock().unwrap();
                let result = match cmd_parts[0] {
                    "Faucet" => state.process_faucet(&sender_addr),
                    // NEW ROUTE: Deploy Program
                    "DeployProgram" if cmd_parts.len() >= 3 => {
                        let prog_id = cmd_parts[1];
                        let bytecode_hex = cmd_parts[2];
                        state.process_deploy_program(&sender_addr, prog_id, bytecode_hex)
                    },
                    "Deploy" if cmd_parts.len() >= 4 => {
                        let ticker = cmd_parts[1].to_string();
                        let name = cmd_parts[2].to_string();
                        let supply = cmd_parts[3].parse::<u64>().unwrap_or(0);
                        let desc = if cmd_parts.len() > 4 { cmd_parts[4].to_string() } else { "".to_string() };
                        state.process_deploy(&sender_addr, ticker, name, supply, desc)
                    },
                    "Transfer" if cmd_parts.len() >= 4 => {
                        let amount = cmd_parts[1].parse().unwrap_or(0);
                        let recipient = cmd_parts[2];
                        let ticker = cmd_parts[3];
                        state.process_transfer(&sender_addr, recipient, ticker, amount, &operator_address)
                    },
                    // MODIFIED ROUTE: Execute only takes program ID now
                    "Execute" if cmd_parts.len() >= 2 => {
                        let prog_id = cmd_parts[1];
                        state.process_execute(&sender_addr, pubkey_hex, prog_id)
                    },
                    "Withdraw" if cmd_parts.len() >= 2 => {
                        let amount = cmd_parts[1].parse().unwrap_or(0);
                        match state.queue_withdrawal(sender_addr.clone(), amount, state::TX_FEE, operator_address.clone()) {
                            Ok((_, record)) => Ok(record),
                            Err(e) => Err(e),
                        }
                    },
                    _ => Err("Unknown command".to_string()),
                };

                match result {
                    Ok(record) => {
                        println!("✅ TX: {} | Type: {}", record.txid, record.tx_type);
                        current_batch_txs.push(record);
                        
                        if current_batch_txs.len() >= 5 {
                             da_adapter.submit_batch(batch_counter, &current_batch_txs).ok();
                             current_batch_txs.clear();
                             batch_counter += 1;
                        }
                        storage.save_state(&state).ok();
                    },
                    Err(e) => println!("❌ TX Fail: {}", e),
                }
            }
        }
    }
}

fn process_deposits(
    app_state: &Arc<Mutex<AppState>>,
    utxos: &[btc_api::Utxo],
    operator_addr: &str,
    storage: &Arc<storage::Storage>
) {
    let mut state = app_state.lock().unwrap();
    let mut changed = false;

    for utxo in utxos {
        if !utxo.status.confirmed { continue; }

        if !state.processed_txs.contains(&utxo.txid) {
            println!("📥 DEPOSIT DETECTED: {} sats", utxo.value);
            state.processed_txs.insert(utxo.txid.clone());
            
            let old_bal = state.get_balance(operator_addr, "BTC");
            state.set_balance(operator_addr, "BTC", old_bal + utxo.value);
            
            state.add_record("Deposit", "BTC", "L1", operator_addr, utxo.value, &utxo.txid);
            changed = true;
        }
    }
    if changed { storage.save_state(&state).ok(); }
}

async fn handle_withdrawals(
    app_state: &Arc<Mutex<AppState>>,
    wallet: &Arc<wallet::LocalWallet>,
    api: &btc_api::BtcApi,
    utxos: &[btc_api::Utxo],
    storage: &Arc<storage::Storage>
) {
    let mut targets = Vec::new();
    let mut pending_ids = Vec::new();

    {
        let state = app_state.lock().unwrap();
        for req in state.withdrawals.values() {
            if req.status == crate::state::WithdrawalStatus::Pending {
                targets.push(withdraw::BatchTarget {
                    address: req.user.clone(),
                    amount: req.amount,
                });
                pending_ids.push(req.id.clone());
                if targets.len() >= 5 { break; } 
            }
        }
    }

    if targets.is_empty() { return; }

    println!("📤 Processing BATCH withdrawal for {} users...", targets.len());

    match withdraw::create_batch_withdrawal_tx(wallet, utxos.to_vec(), targets) {
        Ok(tx) => {
            let tx_hex = serialize_hex(&tx);
            match api.broadcast_tx(tx_hex).await {
                Ok(txid) => {
                    println!("💸 BATCH SENT! TxID: {}", txid);
                    let mut state = app_state.lock().unwrap();
                    for id in pending_ids {
                        if let Some(req) = state.withdrawals.get_mut(&id) {
                            req.status = crate::state::WithdrawalStatus::Completed(txid.clone());
                        }
                    }
                    storage.save_state(&state).ok();
                },
                Err(e) => println!("❌ Batch Broadcast Fail: {}", e),
            }
        },
        Err(e) => println!("❌ Batch Creation Fail: {}", e),
    }
}