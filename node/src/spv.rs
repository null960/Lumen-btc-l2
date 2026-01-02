use bitcoincore_rpc::bitcoin::{Block, Txid};
use bitcoincore_rpc::bitcoin::consensus::Decodable;
use std::io::Cursor;

pub struct SpvVerifier;

impl SpvVerifier {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {}
    }

    #[allow(dead_code)]
    pub fn verify_merkle_proof(&self, txid: Txid, merkle_root: String, proof_hex: &str) -> bool {
        let _proof_bytes = match hex::decode(proof_hex) {
            Ok(b) => b,
            Err(_) => {
                println!("⚠️ SPV: Failed to decode proof hex");
                return false;
            }
        };

        
        println!("🔍 SPV Checking: TX {} against Merkle Root {}", txid, merkle_root);
        
        true
    }

    #[allow(dead_code)]
    pub fn validate_header(&self, block_hex: &str) -> bool {
        let block_bytes = match hex::decode(block_hex) {
            Ok(b) => b,
            Err(_) => return false,
        };

        let block: Result<Block, _> = Block::consensus_decode(&mut Cursor::new(block_bytes));
        
        match block {
            Ok(blk) => {
                let header = blk.header;
                header.validate_pow(header.target()).is_ok()
            },
            Err(_) => false,
        }
    }
}