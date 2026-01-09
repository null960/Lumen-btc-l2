use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct Mempool {
    pub queue: VecDeque<String>,
}

pub fn init_mempool() -> Arc<Mutex<Mempool>> {
    Arc::new(Mutex::new(Mempool {
        queue: VecDeque::new(),
    }))
}