use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppState {
    pub total_transactions: u64,
    pub balances: HashMap<String, u64>,
    pub processed_txs: HashSet<String>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            total_transactions: 0,
            balances: HashMap::new(),
            processed_txs: HashSet::new(),
        }
    }
}

pub fn init_state(saved_state: Option<AppState>) -> Arc<Mutex<AppState>> {
    if let Some(state) = saved_state {
        println!("💾 State Loaded: {} txs processed", state.total_transactions);
        return Arc::new(Mutex::new(state));
    }

    Arc::new(Mutex::new(AppState::new()))
}