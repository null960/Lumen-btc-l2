use std::sync::Arc;
use std::time::Duration;
use bitcoin::consensus::encode::serialize_hex;

mod da_adapter;
mod rpc;
mod mempool;
mod state;
mod storage;
mod wallet;
mod btc_api;
mod withdraw;
mod settlement;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    
    println!("--------------------------------------------------");
    println!("🚀 Lumen-btc-l2: PHASE 6");
    println!("--------------------------------------------------");

    let wallet = wallet::LocalWallet::load_or_generate("keypair.json");
    let api_client = btc_api::BtcApi::new();
    let my_address = wallet.address.clone();
    
    let storage = Arc::new(storage::Storage::new("lumen_db"));
    let saved_state = storage.load_state();
    let app_state = state::init_state(saved_state); 
    let mempool = mempool::init_mempool();
    
    let mut last_settlement_tx_count = app_state.lock().unwrap().total_transactions;
    const SETTLEMENT_INTERVAL: u64 = 3; 

    let rpc_state = Arc::clone(&app_state);
    let rpc_mempool = Arc::clone(&mempool);
    tokio::spawn(async move {
        println!("🌍 Web Terminal running at: http://localhost:3000/wallet");
        rpc::run_server(rpc_state, rpc_mempool).await;
    });

    println!("🟢 Node running. Settlement interval: every {} txs", SETTLEMENT_INTERVAL);

    loop {
        match api_client.get_utxos(&my_address).await {
            Ok(utxos) => {
                let mut state = app_state.lock().unwrap();
                let mut state_changed = false;

                for utxo in utxos {
                    if !state.processed_txs.contains(&utxo.txid) {
                        println!("✅ DEPOSIT: {} sats (Tx: {})", utxo.value, utxo.txid);
                        state.processed_txs.insert(utxo.txid.clone());
                        let user = "0xUser".to_string();
                        let old_bal = *state.balances.get(&user).unwrap_or(&0);
                        state.balances.insert(user.clone(), old_bal + utxo.value);
                        state_changed = true;
                    }
                }
                if state_changed {
                    drop(state);
                    storage.save_state(&app_state.lock().unwrap()).ok();
                }
            }
            Err(_) => {}
        }

        {
            let mut mp = mempool.lock().unwrap();
            while let Some(cmd) = mp.queue.pop_front() {
                println!("⚙️ Processing: {}", cmd);
                let parts: Vec<&str> = cmd.split_whitespace().collect();
                
                let mut tx_success = false;

                if parts.len() == 3 && parts[0].eq_ignore_ascii_case("Transfer") {
                    if let Ok(amount) = parts[1].parse::<u64>() {
                        let recipient = parts[2].to_string();
                        let sender = "0xUser".to_string();
                        let mut state = app_state.lock().unwrap();
                        let sender_bal = *state.balances.get(&sender).unwrap_or(&0);

                        if sender_bal >= amount {
                            state.balances.insert(sender.clone(), sender_bal - amount);
                            let r_bal = *state.balances.get(&recipient).unwrap_or(&0);
                            state.balances.insert(recipient.clone(), r_bal + amount);
                            state.total_transactions += 1;
                            println!("✅ L2 TRANSFER: {} -> {}", amount, recipient);
                            storage.save_state(&state).ok();
                            tx_success = true;
                        } else {
                            println!("❌ TRANSFER FAILED: Insufficient funds");
                        }
                    }
                }
                else if parts.len() == 3 && parts[0].eq_ignore_ascii_case("Withdraw") {
                    if let Ok(amount) = parts[1].parse::<u64>() {
                        let btc_addr = parts[2].to_string();
                        let sender = "0xUser".to_string();
                        let mut state = app_state.lock().unwrap();
                        let sender_bal = *state.balances.get(&sender).unwrap_or(&0);

                        if sender_bal >= amount {
                            println!("⏳ Withdrawing {}...", amount);
                            drop(state); 
                            
                            match api_client.get_utxos(&my_address).await {
                                Ok(utxos) => {
                                    match withdraw::create_withdrawal_tx(&wallet, utxos, btc_addr, amount) {
                                        Ok(tx) => {
                                            let txid_res = api_client.broadcast_tx(serialize_hex(&tx)).await;
                                            match txid_res {
                                                Ok(txid) => {
                                                    println!("🚀 WITHDRAW SENT: {}", txid);
                                                    let mut state = app_state.lock().unwrap();
                                                    state.processed_txs.insert(txid);
                                                    state.balances.insert(sender.clone(), sender_bal - amount);
                                                    state.total_transactions += 1;
                                                    storage.save_state(&state).ok();
                                                    tx_success = true;
                                                }
                                                Err(e) => println!("❌ BROADCAST FAILED: {}", e),
                                            }
                                        }
                                        Err(e) => println!("❌ TX BUILD ERROR: {}", e),
                                    }
                                }
                                Err(e) => println!("❌ API ERROR: {}", e),
                            }
                        }
                    }
                }
                
                if tx_success {
                    let state = app_state.lock().unwrap();
                    let current_txs = state.total_transactions;
                    drop(state);

                    if current_txs - last_settlement_tx_count >= SETTLEMENT_INTERVAL {
                        println!("🔒 Triggering L1 Settlement ({} txs since last)...", current_txs - last_settlement_tx_count);
                        
                        let state_read = app_state.lock().unwrap();
                        let state_hash = settlement::hash_state(&state_read.balances);
                        drop(state_read);
                        
                        println!("#️⃣ State Hash: {}", state_hash);

                        match api_client.get_utxos(&my_address).await {
                            Ok(utxos) => {
                                match settlement::create_settlement_tx(&wallet, utxos, state_hash) {
                                    Ok(tx) => {
                                        match api_client.broadcast_tx(serialize_hex(&tx)).await {
                                            Ok(txid) => {
                                                println!("🏛️ SETTLEMENT CONFIRMED! Tx: {}", txid);
                                                println!("🔗 Proof of State stored in Bitcoin Testnet.");
                                                
                                                last_settlement_tx_count = current_txs;
                                                let mut state = app_state.lock().unwrap();
                                                state.processed_txs.insert(txid);
                                            }
                                            Err(e) => println!("❌ SETTLEMENT FAILED: {}", e),
                                        }
                                    }
                                    Err(e) => println!("❌ SETTLEMENT BUILD ERROR: {}", e),
                                }
                            }
                            Err(_) => {}
                        }
                    }
                }
            }
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}