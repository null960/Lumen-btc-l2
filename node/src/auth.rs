use sha2::{Sha256, Digest};
use hmac::{Hmac, Mac};
use rand::Rng;
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
    Json,
};
use std::sync::{Arc, Mutex};
use serde_json::json;

use crate::state::AppState;

type HmacSha256 = Hmac<Sha256>;

// ── API key format: lm_live_<32 hex chars> ────────────────────────────────────

pub fn generate_key() -> String {
    let bytes: [u8; 16] = rand::thread_rng().gen();
    format!("lm_live_{}", hex::encode(bytes))
}

/// SHA-256 hash of raw key — this is what we store and index on
pub fn hash_key(raw: &str) -> String {
    let mut h = Sha256::new();
    h.update(raw.as_bytes());
    format!("{:x}", h.finalize())
}

/// Display hint: "lm_live_ab12cd34..." (first 16 chars + ellipsis)
pub fn key_hint(raw: &str) -> String {
    format!("{}...", &raw[..raw.len().min(16)])
}

// ── HMAC-SHA256 webhook signature ────────────────────────────────────────────

pub fn hmac_signature(secret: &str, body: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(body.as_bytes());
    format!("sha256={:x}", mac.finalize().into_bytes())
}

/// Constant-time comparison to prevent timing attacks
pub fn verify_signature(body: &str, header: &str, secret: &str) -> bool {
    let expected = hmac_signature(secret, body);
    if expected.len() != header.len() { return false; }
    expected.bytes().zip(header.bytes()).all(|(a, b)| a == b)
}

// ── Auth middleware ───────────────────────────────────────────────────────────

/// Validates API key from `Authorization: Bearer <key>` or `X-Api-Key: <key>`.
/// Injects `MerchantId` extension into request for downstream handlers.
pub async fn require_auth(
    State(state): State<Arc<Mutex<AppState>>>,
    mut req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let raw_key = extract_key(req.headers()).ok_or_else(|| (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "error":   "missing_api_key",
            "message": "Provide key via: Authorization: Bearer <key>  or  X-Api-Key: <key>"
        })),
    ))?;

    let merchant_id = {
        let s = state.lock().unwrap();
        s.merchant_by_key(&raw_key).map(|m| m.id.clone())
    };

    let id = merchant_id.ok_or_else(|| (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "error":   "invalid_api_key",
            "message": "API key not found"
        })),
    ))?;

    req.extensions_mut().insert(MerchantId(id));
    Ok(next.run(req).await)
}

fn extract_key(headers: &axum::http::HeaderMap) -> Option<String> {
    // Authorization: Bearer lm_live_...
    if let Some(v) = headers.get("Authorization") {
        if let Ok(s) = v.to_str() {
            if let Some(key) = s.strip_prefix("Bearer ") {
                return Some(key.trim().to_string());
            }
        }
    }
    // X-Api-Key: lm_live_...
    if let Some(v) = headers.get("X-Api-Key") {
        if let Ok(s) = v.to_str() {
            return Some(s.trim().to_string());
        }
    }
    None
}

// ── Request extension injected by middleware ──────────────────────────────────

#[derive(Clone)]
pub struct MerchantId(pub String);