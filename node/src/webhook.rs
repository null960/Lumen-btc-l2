//! Webhook system — notify apps when payments arrive
//!
//! Apps register a URL. When a Transfer/BuyToken arrives for their address
//! or app_id, the node POSTs a JSON event to that URL.
//!
//! ## Registration
//! POST /api/webhooks/register
//! { "app_id": "my-game", "url": "https://mygame.com/lumen-webhook", "secret": "..." }
//!
//! ## Event format
//! POST https://mygame.com/lumen-webhook
//! X-Lumen-Signature: sha256(secret + body)
//! { "event": "payment", "app_id": "my-game", "from": "tb1q...",
//!   "amount": 1000, "token": "LSAT", "memo": "...", "txid": "..." }

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use chrono::Utc;
use sha2::{Sha256, Digest};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WebhookConfig {
    /// App ID this webhook belongs to
    pub app_id: String,
    /// URL to POST events to
    pub url: String,
    /// HMAC secret for signature verification (optional but recommended)
    pub secret: Option<String>,
    /// Only fire for these event types (None = all)
    pub events: Option<Vec<String>>,
    /// When registered
    pub created_at: i64,
    /// Number of successful deliveries
    pub delivered: u64,
    /// Number of failed deliveries
    pub failed: u64,
    /// Active or not
    pub active: bool,
}

#[derive(Serialize, Debug, Clone)]
pub struct WebhookEvent {
    /// Event type: "payment" | "token_purchase" | "withdrawal_complete"
    pub event: String,
    pub app_id: String,
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub token: String,
    pub memo: Option<String>,
    pub txid: String,
    pub timestamp: i64,
}

#[derive(Clone)]
pub struct WebhookManager {
    /// app_id -> WebhookConfig
    configs: Arc<Mutex<HashMap<String, WebhookConfig>>>,
    /// HTTP client for delivery
    client: reqwest::Client,
}

impl WebhookManager {
    pub fn new() -> Self {
        Self {
            configs: Arc::new(Mutex::new(HashMap::new())),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Register or update a webhook for an app
    pub fn register(&self, config: WebhookConfig) {
        let mut map = self.configs.lock().unwrap();
        map.insert(config.app_id.clone(), config);
    }

    /// Remove a webhook
    pub fn unregister(&self, app_id: &str) {
        let mut map = self.configs.lock().unwrap();
        map.remove(app_id);
    }

    /// Get all registered webhooks (for status endpoint)
    pub fn list(&self) -> Vec<WebhookConfig> {
        let map = self.configs.lock().unwrap();
        map.values().cloned().collect()
    }

    /// Get webhook for specific app
    pub fn get(&self, app_id: &str) -> Option<WebhookConfig> {
        let map = self.configs.lock().unwrap();
        map.get(app_id).cloned()
    }

    /// Fire event for a specific app (non-blocking — spawns async task)
    pub fn fire(&self, event: WebhookEvent) {
        let config = match self.get(&event.app_id) {
            Some(c) if c.active => c,
            _ => return, // no webhook registered or inactive
        };

        // Check event filter
        if let Some(ref allowed) = config.events {
            if !allowed.contains(&event.event) { return; }
        }

        let client = self.client.clone();
        let configs = self.configs.clone();

        tokio::spawn(async move {
            let body = serde_json::to_string(&event).unwrap_or_default();

            // HMAC-SHA256 signature header
            let signature = if let Some(ref secret) = config.secret {
                let mut h = Sha256::new();
                h.update(secret.as_bytes());
                h.update(body.as_bytes());
                format!("sha256={:x}", h.finalize())
            } else {
                "unsigned".to_string()
            };

            let result = client
                .post(&config.url)
                .header("Content-Type", "application/json")
                .header("X-Lumen-Signature", &signature)
                .header("X-Lumen-Event", &event.event)
                .header("User-Agent", "Lumen-Network/2.0")
                .body(body)
                .send()
                .await;

            let mut map = configs.lock().unwrap();
            if let Some(cfg) = map.get_mut(&event.app_id) {
                match result {
                    Ok(resp) if resp.status().is_success() => {
                        cfg.delivered += 1;
                        println!("🔔 Webhook delivered: {} → {} ({})",
                            event.event, cfg.url, resp.status());
                    },
                    Ok(resp) => {
                        cfg.failed += 1;
                        println!("⚠️  Webhook failed: {} → {} ({})",
                            event.event, cfg.url, resp.status());
                    },
                    Err(e) => {
                        cfg.failed += 1;
                        println!("⚠️  Webhook error: {} → {}: {}", event.event, cfg.url, e);
                    }
                }
            }
        });
    }

    /// Fire payment event — call after successful Transfer
    pub fn on_payment(&self, app_id: &str, from: &str, to: &str, amount: u64, memo: Option<String>, txid: &str) {
        self.fire(WebhookEvent {
            event: "payment".to_string(),
            app_id: app_id.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            amount,
            token: "LSAT".to_string(),
            memo,
            txid: txid.to_string(),
            timestamp: Utc::now().timestamp(),
        });
    }

    /// Fire token purchase event — call after BuyToken
    #[allow(dead_code)]
    pub fn on_token_purchase(&self, app_id: &str, buyer: &str, lsat_spent: u64, tokens_received: u64, token_name: &str, txid: &str) {
        self.fire(WebhookEvent {
            event: "token_purchase".to_string(),
            app_id: app_id.to_string(),
            from: buyer.to_string(),
            to: app_id.to_string(),
            amount: tokens_received,
            token: token_name.to_string(),
            memo: Some(format!("Spent {} LSAT", lsat_spent)),
            txid: txid.to_string(),
            timestamp: Utc::now().timestamp(),
        });
    }
}