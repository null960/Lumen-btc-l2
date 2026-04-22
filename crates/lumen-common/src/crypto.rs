use sha2::{Sha256, Digest};
use hmac::{Hmac, Mac};
use rand::Rng;

type HmacSha256 = Hmac<Sha256>;

pub fn generate_key() -> String {
    let bytes: [u8; 16] = rand::thread_rng().gen();
    format!("lm_live_{}", hex::encode(bytes))
}

pub fn hash_key(raw: &str) -> String {
    let mut h = Sha256::new();
    h.update(raw.as_bytes());
    format!("{:x}", h.finalize())
}

pub fn key_hint(raw: &str) -> String {
    format!("{}...", &raw[..raw.len().min(16)])
}

pub fn hmac_signature(secret: &str, body: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(body.as_bytes());
    format!("sha256={:x}", mac.finalize().into_bytes())
}