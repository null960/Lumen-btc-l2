use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TxLog {
    pub txid: String,
    pub amount_sats: u64,
    pub to: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppState {
    pub balances: HashMap<String, u64>,
    pub total_transactions: u64,
    #[serde(default)] 
    pub processed_txs: HashSet<String>,
    #[serde(default)]
    pub history: Vec<TxLog>, 
}

impl AppState {
    pub fn new() -> Self {
        Self {
            balances: HashMap::new(),
            total_transactions: 0,
            processed_txs: HashSet::new(),
            history: Vec::new(),
        }
    }
}

pub type SharedState = Arc<Mutex<AppState>>;

pub fn init_state(saved_state: Option<AppState>) -> SharedState {
    if let Some(s) = saved_state {
        Arc::new(Mutex::new(s))
    } else {
        Arc::new(Mutex::new(AppState::new()))
    }
}