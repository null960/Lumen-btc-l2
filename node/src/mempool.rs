use std::sync::{Arc, Mutex};
use std::collections::VecDeque;
use serde::{Deserialize, Serialize};
use solana_sdk::signature::Signature;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L2Transaction {
    pub sender: String,    // Public Key (Base58)
    pub instruction: String, 
    pub amount: u64,
    pub signature: String, // Ed25519 Signature
}

impl L2Transaction {
    /// Verifies that the signature is valid for the given instruction and sender
    pub fn verify_signature(&self) -> bool {
        let pubkey = match Pubkey::from_str(&self.sender) {
            Ok(pk) => pk,
            Err(_) => return false,
        };
        
        let sig = match Signature::from_str(&self.signature) {
            Ok(s) => s,
            Err(_) => return false,
        };

        // In SVM, we verify the signature against the message bytes
        sig.verify(&pubkey.to_bytes(), self.instruction.as_bytes())
    }
}

pub type SharedMempool = Arc<Mutex<VecDeque<L2Transaction>>>;

pub fn init_mempool() -> SharedMempool {
    Arc::new(Mutex::new(VecDeque::new()))
}