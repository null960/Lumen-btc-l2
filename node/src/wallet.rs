use bitcoin::network::constants::Network;
use bitcoin::secp256k1::{Secp256k1, SecretKey, Message, PublicKey}; 
use bitcoin::secp256k1::ecdsa::{RecoverableSignature, RecoveryId};
use bitcoin::{Address, PrivateKey};
use bitcoin::hashes::{sha256d, Hash};
use base64::{Engine as _, engine::general_purpose};
use rand::thread_rng;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::str::FromStr;

const CURRENT_NETWORK: Network = Network::Testnet;

#[derive(Serialize, Deserialize, Debug)]
pub struct LocalWallet {
    pub secret_wif: String,
    pub address: String,
}

impl LocalWallet {
    pub fn load_or_generate(file_path: &str) -> Self {
        if Path::new(file_path).exists() {
            let data = fs::read_to_string(file_path).expect("Read fail");
            serde_json::from_str(&data).expect("JSON fail")
        } else {
            let wallet = Self::new_random();
            wallet.save(file_path);
            wallet
        }
    }

    pub fn new_random() -> Self {
        let secp = Secp256k1::new();
        let secret_key = SecretKey::new(&mut thread_rng());
        let private_key = PrivateKey::new(secret_key, CURRENT_NETWORK);
        let public_key = private_key.public_key(&secp);
        let address = Address::p2pkh(&public_key, CURRENT_NETWORK);
        Self { secret_wif: private_key.to_wif(), address: address.to_string() }
    }

    pub fn save(&self, path: &str) {
        let json = serde_json::to_string_pretty(self).unwrap();
        fs::write(path, json).ok();
    }

    pub fn get_address_obj(&self) -> Address {
        Address::from_str(&self.address).unwrap().assume_checked()
    }
}

pub fn pubkey_to_address(pubkey_hex: &str) -> Option<String> {
    // FIX: Parsing directly into bitcoin::PublicKey instead of secp256k1::PublicKey
    let btc_pubkey = bitcoin::PublicKey::from_str(pubkey_hex).ok()?;
    Some(Address::p2wpkh(&btc_pubkey, CURRENT_NETWORK).ok()?.to_string())
}

pub fn verify_signature(message: &str, signature_base64: &str, pubkey_hex: &str) -> bool {
    let secp = Secp256k1::verification_only();
    let pubkey = match PublicKey::from_str(pubkey_hex) { Ok(pk) => pk, Err(_) => return false };
    let sig_bytes = match general_purpose::STANDARD.decode(signature_base64) { Ok(b) => b, Err(_) => return false };
    if sig_bytes.len() != 65 { return false; }

    let rec_id = match RecoveryId::from_i32(((sig_bytes[0] - 27) & 3) as i32) { Ok(id) => id, Err(_) => return false };
    let rec_sig = match RecoverableSignature::from_compact(&sig_bytes[1..], rec_id) { Ok(s) => s, Err(_) => return false };

    let mut data = Vec::new();
    data.extend_from_slice(b"\x18Bitcoin Signed Message:\n");
    data.push(message.len() as u8);
    data.extend_from_slice(message.as_bytes());

    let msg_hash = sha256d::Hash::hash(&data);
    let msg = Message::from_slice(msg_hash.as_byte_array()).unwrap();

    match secp.recover_ecdsa(&msg, &rec_sig) {
        Ok(recovered) => recovered == pubkey,
        Err(_) => false,
    }
}