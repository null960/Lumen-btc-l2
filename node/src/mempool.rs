use std::sync::{Arc, Mutex};
use std::collections::VecDeque;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L2Transaction {
    pub sender: String,    // Users public key
    pub instruction: String, // Action name
    pub amount: u64,       // Value
    pub signature: String, // Proof
}

// Safe storage for structured transactions
pub type SharedMempool = Arc<Mutex<VecDeque<L2Transaction>>>;

pub fn init_mempool() -> SharedMempool {
    Arc::new(Mutex::new(VecDeque::new()))
}