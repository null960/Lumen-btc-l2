use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L2Transaction {
    pub sender: String,
    pub instruction: String,
    pub signature: String,
    pub timestamp: u64, 
    pub pubkey: String,
}

impl L2Transaction {
    #[allow(dead_code)]
    pub fn verify_signature(&self) -> bool {
        !self.signature.is_empty()
    }
}

pub type SharedMempool = Arc<Mutex<VecDeque<L2Transaction>>>;

pub fn init_mempool() -> SharedMempool {
    Arc::new(Mutex::new(VecDeque::new()))
}