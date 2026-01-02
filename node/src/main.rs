use std::sync::Arc;
use std::time::Duration;
use std::str::FromStr;
use bitcoincore_rpc::{Auth, Client, RpcApi};

mod da_adapter;
mod rpc;
mod mempool;
mod state;
mod storage;
mod bridge;
mod spv;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    
    println!("--------------------------------------------------");
    println!("🚀 Lumen-btc-l2: Phase 6");
    println!("--------------------------------------------------");

    let storage = Arc::new(storage::Storage::new("lumen_db"));
    let saved = storage.load_state();
    let app_state = state::init_state(saved); 

    let mempool = mempool::init_mempool();
    let _da_layer = da_adapter::BitcoinDAAdapter::new("http://localhost:26659");

    let btc_rpc = Client::new(
        "http://127.0.0.1:18443",
        Auth::UserPass("user".to_string(), "password".to_string()),
    ).expect("Failed to connect to Bitcoin Core");

    let bridge = bridge::BitcoinBridge::new(
        "http://127.0.0.1:18443", 
        Arc::clone(&app_state),
        Arc::clone(&storage)
    );

    let rpc_mempool = Arc::clone(&mempool);
    let rpc_state = Arc::clone(&app_state);
    tokio::spawn(async move {
        rpc::start_rpc_server(rpc_mempool, rpc_state).await;
    });

    {
        let s = app_state.lock().unwrap();
        println!("🧠 VM Ready. Accounts: {}", s.balances.len());
    }

    loop {
        tokio::time::sleep(Duration::from_secs(3)).await;
        
        bridge.sync();

        let mut q = mempool.lock().unwrap();
        if !q.is_empty() {
            println!("⚡ Processing batch of {} commands...", q.len());
            
            let mut state = app_state.lock().unwrap();
            let mut save_needed = false;

            for tx in q.iter() {
                state.total_transactions += 1;
                let parts: Vec<&str> = tx.instruction.split_whitespace().collect();
                
                if parts.is_empty() { continue; }

                match parts[0] {
                    "Transfer" => {
                        if parts.len() >= 3 {
                            if let Ok(amount) = parts[1].parse::<u64>() {
                                let sender = &tx.sender;
                                let recipient = parts[2];
                                let sender_bal = state.balances.entry(sender.clone()).or_insert(0);
                                
                                if *sender_bal >= amount {
                                    *sender_bal -= amount;
                                    *state.balances.entry(recipient.to_string()).or_insert(0) += amount;
                                    println!("💸 TRANSFER: {} sats from {} to {}", amount, sender, recipient);
                                    save_needed = true;
                                } else {
                                    println!("⛔ Transfer failed: Insufficient funds for {}", sender);
                                }
                            }
                        }
                    },
                    "Withdraw" => {
                        if parts.len() >= 3 {
                            if let Ok(amount) = parts[1].parse::<u64>() {
                                let sender = &tx.sender;
                                let btc_addr_str = parts[2]; 
                                let sender_bal = state.balances.entry(sender.clone()).or_insert(0);

                                if *sender_bal >= amount {
                                    let btc_address = match bitcoincore_rpc::bitcoin::Address::from_str(btc_addr_str) {
                                        Ok(addr) => addr.assume_checked(),
                                        Err(e) => {
                                            println!("⛔ Invalid BTC Address '{}': {}", btc_addr_str, e);
                                            continue;
                                        }
                                    };

                                    *sender_bal -= amount;
                                    save_needed = true;

                                    let btc_val = amount as f64 / 100_000_000.0;
                                    
                                    match btc_rpc.send_to_address(
                                        &btc_address,
                                        bitcoincore_rpc::bitcoin::Amount::from_btc(btc_val).unwrap(),
                                        None, None, None, None, None, None
                                    ) {
                                        Ok(txid) => println!("📤 WITHDRAW SUCCESS: Sent {} BTC to {}. TxId: {}", btc_val, btc_addr_str, txid),
                                        Err(e) => {
                                            println!("⚠️ WITHDRAW FAILED (Refunded): {}", e);
                                            *sender_bal += amount; 
                                        }
                                    }
                                } else {
                                    println!("⛔ Withdraw failed: Insufficient funds");
                                }
                            }
                        }
                    },
                    "Faucet" => {
                        let target_btc_addr_str = if parts.len() > 1 { parts[1] } else { &tx.sender };
                        let faucet_amount = 0.1;

                        let btc_address = match bitcoincore_rpc::bitcoin::Address::from_str(target_btc_addr_str) {
                            Ok(addr) => addr.assume_checked(),
                            Err(e) => {
                                println!("⛔ Invalid Faucet Address '{}': {}", target_btc_addr_str, e);
                                continue;
                            }
                        };
                        
                        match btc_rpc.send_to_address(
                            &btc_address,
                            bitcoincore_rpc::bitcoin::Amount::from_btc(faucet_amount).unwrap(),
                            None, None, None, None, None, None
                        ) {
                             Ok(txid) => println!("🚰 FAUCET: Sent {} BTC to {}. TxId: {}", faucet_amount, target_btc_addr_str, txid),
                             Err(e) => println!("⚠️ Faucet Error: {}", e),
                        }
                    },
                    _ => println!("❓ Unknown command: {}", parts[0]),
                }
            }

            if save_needed {
                let _ = storage.save_state(&state);
                println!("💾 State saved.");
            }

            q.clear();
        }
    }
}