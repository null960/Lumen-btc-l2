use std::fs;
use std::path::Path;
use std::error::Error;
use bitcoin::hashes::{sha256, Hash}; 
use crate::state::TxRecord;

pub struct BitcoinDAAdapter {
    storage_path: String,
}

impl BitcoinDAAdapter {
    pub fn new(storage_path: &str) -> Self {
        if !Path::new(storage_path).exists() {
            fs::create_dir(storage_path).unwrap();
        }
        Self {
            storage_path: storage_path.to_string(),
        }
    }

    pub fn submit_batch(&self, batch_id: u64, txs: &Vec<TxRecord>) -> Result<String, Box<dyn Error>> {
        let json_data = serde_json::to_string(txs)?;

        let hash = sha256::Hash::hash(json_data.as_bytes());
        let hash_hex = hash.to_string();

        let filename = format!("{}/batch_{}_{}.json", self.storage_path, batch_id, hash_hex);
        fs::write(&filename, json_data)?;

        println!("📦 [DA Layer] Batch #{} saved to {}", batch_id, filename);
        Ok(hash_hex)
    }
}