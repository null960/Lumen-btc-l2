use std::sync::{Arc, Mutex};
use serde::{Serialize, Deserialize};

// The "Database" structure in memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    // Keeps track of the total number of txs processed by the L2
    pub total_transactions: u64, 
    // A simple counter value managed by our "Smart Contract"
    pub smart_contract_counter: u64, 
}

impl AppState {
    pub fn new() -> Self {
        Self {
            total_transactions: 0,
            smart_contract_counter: 0,
        }
    }
}

// Thread-safe wrapper to share state between RPC and Sequencer
pub type SharedState = Arc<Mutex<AppState>>;

pub fn init_state() -> SharedState {
    Arc::new(Mutex::new(AppState::new()))
}