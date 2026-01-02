use bitcoincore_rpc::{Auth, Client, RpcApi};
use bitcoincore_rpc::json::GetTransactionResultDetailCategory;
use crate::state::{SharedState, TxLog};
use crate::storage::Storage;
use crate::spv::SpvVerifier;
use std::sync::Arc;

pub struct BitcoinBridge {
    rpc: Client,
    state: SharedState,
    storage: Arc<Storage>,
    spv: SpvVerifier,
}

impl BitcoinBridge {
    pub fn new(url: &str, state: SharedState, storage: Arc<Storage>) -> Self {
        let rpc = Client::new(
            url,
            Auth::UserPass("user".to_string(), "password".to_string()),
        ).expect("Failed to create RPC client");
        
        Self { 
            rpc, 
            state, 
            storage,
            spv: SpvVerifier::new(),
        }
    }

    pub fn sync(&self) {
        match self.rpc.list_transactions(None, Some(10), None, Some(true)) {
            Ok(txs) => {
                let mut state = self.state.lock().unwrap();
                let mut changed = false;

                for tx in txs {
                    let info = &tx.info;
                    let detail = &tx.detail;

                    if detail.category != GetTransactionResultDetailCategory::Receive { continue; }
                    if detail.amount.to_btc() <= 0.0 || info.confirmations == 0 { continue; }

                    let txid_str = info.txid.to_string();
                    
                    if !state.processed_txs.contains(&txid_str) {
                        
                        if let Some(blockhash) = info.blockhash {
                            let header_info = match self.rpc.get_block_header_info(&blockhash) {
                                Ok(h) => h,
                                Err(e) => {
                                    println!("⚠️ SPV Error: Could not get header: {}", e);
                                    continue;
                                }
                            };

                            let proof_bytes = match self.rpc.get_tx_out_proof(&[info.txid], Some(&blockhash)) {
                                Ok(p) => p,
                                Err(e) => {
                                    println!("⚠️ SPV Error: Could not get proof: {}", e);
                                    continue; 
                                }
                            };
                            let proof_hex = hex::encode(proof_bytes);

                            let merkle_root = header_info.merkle_root.to_string();
                            let is_valid = self.spv.verify_merkle_proof(info.txid, merkle_root, &proof_hex);

                            if !is_valid {
                                println!("⛔ SPV ALARM: Transaction {} failed verification! Ignoring.", txid_str);
                                continue;
                            }
                        } else {
                            println!("⚠️ Transaction confirmed but has no blockhash? Skipping.");
                            continue;
                        }

                        let label_opt = detail.label.clone();
                        let target_l2_address = match label_opt {
                            Some(label) if label.starts_with("0x") => label,
                            _ => "0x107cb97206f84fa3".to_string(),
                        };

                        let btc_amount = detail.amount.to_btc();
                        let lbtc_sats = (btc_amount * 100_000_000.0) as u64;

                        *state.balances.entry(target_l2_address.clone()).or_insert(0) += lbtc_sats;
                        
                        state.processed_txs.insert(txid_str.clone());
                        state.history.push(TxLog {
                            txid: txid_str.clone(),
                            amount_sats: lbtc_sats,
                            to: target_l2_address.clone(),
                        });

                        println!("✅ LBTC DEPOSIT (SPV Verified): {} sats credited to {}", lbtc_sats, target_l2_address);
                        changed = true;
                    }
                }

                if changed {
                    if let Err(e) = self.storage.save_state(&state) {
                        println!("⚠️ Storage Error: {}", e);
                    } else {
                        println!("💾 State synced to disk.");
                    }
                }
            },
            Err(e) => println!("⚠️ Bridge Error: {}", e),
        }
    }
}