use std::sync::Arc;
use std::time::Duration;
use std::str::FromStr;
use bitcoin::consensus::encode::serialize_hex;
use bitcoin::{Address, PublicKey, Network};
use tokio::sync::mpsc;
use chrono::Utc; 

mod da_adapter;
mod rpc;
mod state;
mod storage;
mod wallet;
mod btc_api;
mod withdraw;
mod settlement;

const TX_FEE: u64 = 100;

#[derive(Debug)]
pub enum NetworkEvent {
    Transaction(String),
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    
    let wallet = wallet::LocalWallet::load_or_generate("keypair.json");
    let operator_address = wallet.address.clone();

    println!("--------------------------------------------------");
    println!("🚀 Lumen L2: Phase 6: Public Testnet (In progress) 🚧");
    println!("💰 Fees: {} sats | 🚰 Faucet: Active", TX_FEE);
    println!("👤 Operator Node: {}", operator_address);
    println!("--------------------------------------------------");

    let api_client = btc_api::BtcApi::new();
    let storage = Arc::new(storage::Storage::new("lumen_db"));
    let da_adapter = da_adapter::BitcoinDAAdapter::new("lumen_da");

    let saved_state = storage.load_state();
    let app_state = state::init_state(saved_state); 
    
    let (tx_channel, mut rx_channel) = mpsc::channel::<NetworkEvent>(10000);

    let mut last_settlement_tx_count = app_state.lock().unwrap().total_transactions;
    let mut current_batch_txs: Vec<state::TxRecord> = Vec::new();
    let mut batch_counter = 1;
    const SETTLEMENT_INTERVAL: u64 = 3; 

    let rpc_state = Arc::clone(&app_state);
    let rpc_tx = tx_channel.clone();
    let rpc_addr_clone = operator_address.clone();
    
    tokio::spawn(async move {
        rpc::run_server(rpc_state, rpc_tx, rpc_addr_clone).await;
    });

    loop {
        tokio::select! {
            Some(event) = rx_channel.recv() => {
                match event {
                    NetworkEvent::Transaction(raw_cmd) => {
                        let parts: Vec<&str> = raw_cmd.split('|').collect();
                        if parts.len() == 4 && parts[0] == "SIGNED_CMD" {
                            let real_cmd = parts[1];
                            let pubkey_hex = parts[3];

                            if let Ok(pk) = PublicKey::from_str(pubkey_hex) {
                                if let Ok(sender_addr_obj) = Address::p2wpkh(&pk, Network::Testnet) {
                                    let sender = sender_addr_obj.to_string();
                                    let cmd_parts: Vec<&str> = real_cmd.split_whitespace().collect();
                                    
                                    // === 1. FAUCET ===
                                    if cmd_parts.len() == 1 && cmd_parts[0].eq_ignore_ascii_case("Faucet") {
                                        let mut state = app_state.lock().unwrap();
                                        let last_claim = *state.last_faucet_claim.get(&sender).unwrap_or(&0);
                                        let now = Utc::now().timestamp();

                                        if now - last_claim >= 600 {
                                            let amount = 1000;
                                            let old_bal = *state.balances.get(&sender).unwrap_or(&0);
                                            state.balances.insert(sender.clone(), old_bal + amount);
                                            
                                            state.last_faucet_claim.insert(sender.clone(), now);

                                            state.total_transactions += 1;
                                            let rec = state.add_record("Faucet", "Lumen_System", &sender, amount, "L2_AirDrop");
                                            current_batch_txs.push(rec);

                                            println!("🚰 FAUCET CLAIM: {} (+1000 sats)", sender);
                                            storage.save_state(&state).ok();
                                        } else {
                                            println!("⏳ FAUCET LIMIT: {} must wait", sender);
                                        }
                                    }

                                    // === 2. TRANSFER ===
                                    else if cmd_parts.len() == 3 && cmd_parts[0].eq_ignore_ascii_case("Transfer") {
                                        if let Ok(amount) = cmd_parts[1].parse::<u64>() {
                                            let recipient = cmd_parts[2].to_string();
                                            let mut state = app_state.lock().unwrap();
                                            
                                            let sender_bal = *state.balances.get(&sender).unwrap_or(&0);
                                            let total_cost = amount + TX_FEE;

                                            if sender_bal >= total_cost {
                                                state.balances.insert(sender.clone(), sender_bal - total_cost);
                                                let r_bal = *state.balances.get(&recipient).unwrap_or(&0);
                                                state.balances.insert(recipient.clone(), r_bal + amount);
                                                
                                                let op_bal = *state.balances.get(&operator_address).unwrap_or(&0);
                                                state.balances.insert(operator_address.clone(), op_bal + TX_FEE);

                                                state.total_transactions += 1;
                                                let rec = state.add_record("Transfer", &sender, &recipient, amount, "L2_Signed");
                                                current_batch_txs.push(rec);

                                                println!("✅ TRANSFER: {} -> {} (Fee Paid)", sender, recipient);
                                                storage.save_state(&state).ok();
                                            } else {
                                                println!("⛔ LOW BALANCE: {}", sender);
                                            }
                                        }
                                    }

                                    // === 3. WITHDRAW ===
                                    else if cmd_parts.len() == 2 && cmd_parts[0].eq_ignore_ascii_case("Withdraw") {
                                        if let Ok(amount) = cmd_parts[1].parse::<u64>() {
                                            let mut state = app_state.lock().unwrap();
                                            let sender_bal = *state.balances.get(&sender).unwrap_or(&0);
                                            let total_cost = amount + TX_FEE; 

                                            if sender_bal >= total_cost {
                                                state.balances.insert(sender.clone(), sender_bal - total_cost);
                                                let op_bal = *state.balances.get(&operator_address).unwrap_or(&0);
                                                state.balances.insert(operator_address.clone(), op_bal + TX_FEE);

                                                state.total_transactions += 1;
                                                let rec = state.add_record("Withdraw", &sender, "Bitcoin L1", amount, "Processing");
                                                current_batch_txs.push(rec);
                                                
                                                println!("🔄 WITHDRAW REQUEST: {}", sender);
                                                storage.save_state(&state).ok();
                                                drop(state);
                                                
                                                if let Ok(utxos) = api_client.get_utxos(&operator_address).await {
                                                    if let Ok(tx) = withdraw::create_withdrawal_tx(&wallet, utxos, sender.clone(), amount) {
                                                        match api_client.broadcast_tx(serialize_hex(&tx)).await {
                                                            Ok(txid) => println!("💸 L1 TX: {}", txid),
                                                            Err(e) => println!("❌ L1 ERROR: {}", e),
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            _ = tokio::time::sleep(Duration::from_secs(2)) => {
                // Deposits Check
                if let Ok(utxos) = api_client.get_utxos(&operator_address).await {
                    let mut state = app_state.lock().unwrap();
                    let mut changed = false;
                    for utxo in utxos {
                        let already_processed = state.processed_txs.contains(&utxo.txid);
                        let in_history = state.history.iter().any(|h| h.txid == utxo.txid && h.tx_type == "Deposit");
                        if !already_processed && !in_history {
                            println!("✅ DEPOSIT: {} sats", utxo.value);
                            state.processed_txs.insert(utxo.txid.clone());
                            let user = operator_address.clone(); 
                            let old = *state.balances.get(&user).unwrap_or(&0);
                            state.balances.insert(user.clone(), old + utxo.value);
                            state.add_record("Deposit", "Bitcoin L1", &user, utxo.value, &utxo.txid);
                            if let Some(r) = state.history.last() { current_batch_txs.push(r.clone()); }
                            changed = true;
                        }
                    }
                    if changed { storage.save_state(&state).ok(); }
                }

                // Settlement Check
                let state = app_state.lock().unwrap();
                let total = state.total_transactions;
                drop(state);

                if (total - last_settlement_tx_count >= SETTLEMENT_INTERVAL) && !current_batch_txs.is_empty() {
                    println!("🔒 Batching {} transactions...", current_batch_txs.len());
                    if let Ok(hash) = da_adapter.submit_batch(batch_counter, &current_batch_txs) {
                        println!("📦 Batch Saved: {}", hash);
                        if let Ok(utxos) = api_client.get_utxos(&operator_address).await {
                             if let Ok(tx) = settlement::create_settlement_tx(&wallet, utxos, hash) {
                                if let Ok(txid) = api_client.broadcast_tx(serialize_hex(&tx)).await {
                                    println!("🏛️  ANCHORED: {}", txid);
                                    let mut s = app_state.lock().unwrap();
                                    s.processed_txs.insert(txid.clone());
                                    s.add_record("Settlement", "Sequencer", "L1", 0, &txid);
                                    storage.save_state(&s).ok();
                                    last_settlement_tx_count = total;
                                    current_batch_txs.clear();
                                    batch_counter += 1;
                                }
                             }
                        }
                    }
                }
            }
        }
    }
}