use bitcoin::network::constants::Network;
use bitcoin::secp256k1::{Secp256k1, SecretKey}; 
use bitcoin::{Address, PrivateKey};
use rand::thread_rng;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::str::FromStr;
use std::fmt;

const CURRENT_NETWORK: Network = Network::Testnet;

#[derive(Serialize, Deserialize, Debug)]
pub struct LocalWallet {
    pub secret_wif: String,
    pub address: String,
}

impl LocalWallet {
    pub fn load_or_generate(file_path: &str) -> Self {
        if Path::new(file_path).exists() {
            let data = fs::read_to_string(file_path).expect("Unable to read keypair file");
            let wallet: LocalWallet = serde_json::from_str(&data).expect("Invalid keypair JSON");
            println!("🔑 Loaded existing wallet: {}", wallet.address);
            return wallet;
        } else {
            println!("⚙️ Generating new Testnet wallet...");
            let new_wallet = Self::new_random();
            new_wallet.save(file_path);
            return new_wallet;
        }
    }

    pub fn new_random() -> Self {
        let secp = Secp256k1::new();
        
        let secret_key = SecretKey::new(&mut thread_rng());
        
        let private_key = PrivateKey::new(secret_key, CURRENT_NETWORK);
        
        let public_key = private_key.public_key(&secp);
        
        let address = Address::p2pkh(&public_key, CURRENT_NETWORK);

        Self {
            secret_wif: private_key.to_wif(),
            address: address.to_string(),
        }
    }

    pub fn save(&self, path: &str) {
        let json = serde_json::to_string_pretty(self).unwrap();
        fs::write(path, json).expect("Unable to save keypair");
        println!("💾 Wallet saved to {}", path);
    }

    pub fn get_address_obj(&self) -> Address {
        Address::from_str(&self.address).unwrap().assume_checked()
    }
}

impl fmt::Display for LocalWallet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Address: {}\nPrivate Key (WIF): {} (KEEP SAFE!)", self.address, self.secret_wif)
    }
}