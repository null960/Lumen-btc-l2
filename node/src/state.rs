use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use chrono::Utc; 

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TxRecord {
    pub tx_type: String,
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub txid: String,
    pub timestamp: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppState {
    pub total_transactions: u64,
    pub balances: HashMap<String, u64>,
    pub processed_txs: HashSet<String>,
    pub history: Vec<TxRecord>,
    
    pub last_faucet_claim: HashMap<String, i64>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            total_transactions: 0,
            balances: HashMap::new(),
            processed_txs: HashSet::new(),
            history: Vec::new(),
            last_faucet_claim: HashMap::new(),
        }
    }

    pub fn add_record(&mut self, tx_type: &str, from: &str, to: &str, amount: u64, txid: &str) -> TxRecord {
        let record = TxRecord {
            tx_type: tx_type.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            amount,
            txid: txid.to_string(),
            timestamp: Utc::now().timestamp(),
        };
        self.history.push(record.clone());
        if self.history.len() > 100 {
            self.history.remove(0);
        }
        record
    }
}

pub fn init_state(saved_state: Option<AppState>) -> Arc<Mutex<AppState>> {
    if let Some(state) = saved_state {
        println!("💾 State Loaded: {} txs in history", state.history.len());
        return Arc::new(Mutex::new(state));
    }
    Arc::new(Mutex::new(AppState::new()))
}