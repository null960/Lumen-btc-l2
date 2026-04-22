use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;
use chrono::Utc;
use tracing::{info, warn};

use crate::auth::hmac_signature;

// ── Event types ───────────────────────────────────────────────────────────────

pub struct WebhookEvent<'a> {
    pub merchant_id: &'a str,
    pub url:         &'a str,
    pub secret:      Option<&'a str>,
    pub event:       &'static str,
    pub data:        Value,
}

// ── Manager ───────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct WebhookManager {
    client: Client,
}

impl WebhookManager {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .user_agent("Lumen-Webhooks/2.0")
                .build()
                .unwrap_or_default(),
        }
    }

    /// Fire a webhook — non-blocking, retries up to 3× with backoff.
    /// Payload is signed with HMAC-SHA256 when secret is provided.
    pub fn fire(&self, event: WebhookEvent<'_>) {
        let client      = self.client.clone();
        let url         = event.url.to_string();
        let secret      = event.secret.map(|s| s.to_string());
        let merchant_id = event.merchant_id.to_string();

        let payload = json!({
            "id":          uuid::Uuid::new_v4().to_string(),
            "event":       event.event,
            "api_version": "2024-01",
            "created":     Utc::now().timestamp(),
            "data":        event.data,
        });

        tokio::spawn(async move {
            deliver(&client, &url, secret.as_deref(), &payload, &merchant_id).await;
        });
    }

    // ── Typed event helpers ───────────────────────────────────────────────────

    pub fn payment_received(
        &self,
        merchant_id: &str,
        url: &str,
        secret: Option<&str>,
        invoice_id: &str,
        amount_sats: u64,
        payment_hash: &str,
    ) {
        self.fire(WebhookEvent {
            merchant_id,
            url,
            secret,
            event: "payment.received",
            data: json!({
                "invoice_id":   invoice_id,
                "amount_sats":  amount_sats,
                "payment_hash": payment_hash,
                "paid_at":      Utc::now().to_rfc3339(),
            }),
        });
    }
}

// ── Delivery with retry ───────────────────────────────────────────────────────

async fn deliver(
    client:      &Client,
    url:         &str,
    secret:      Option<&str>,
    payload:     &Value,
    merchant_id: &str,
) {
    let body = match serde_json::to_string(payload) {
        Ok(b) => b,
        Err(e) => { warn!("Webhook serialize error: {}", e); return; }
    };

    for attempt in 1u32..=3 {
        let mut req = client.post(url)
            .header("Content-Type",      "application/json")
            .header("X-Lumen-Event",     payload["event"].as_str().unwrap_or(""))
            .header("X-Lumen-Delivery",  payload["id"].as_str().unwrap_or(""));

        if let Some(sec) = secret {
            req = req.header("X-Lumen-Signature", hmac_signature(sec, &body));
        }

        match req.body(body.clone()).send().await {
            Ok(r) if r.status().is_success() => {
                info!(merchant = %merchant_id, event = %payload["event"], attempt, "Webhook delivered");
                return;
            }
            Ok(r) => warn!(merchant = %merchant_id, status = %r.status(), attempt, "Webhook non-2xx"),
            Err(e) => warn!(merchant = %merchant_id, error = %e, attempt, "Webhook error"),
        }

        if attempt < 3 {
            tokio::time::sleep(Duration::from_secs(2u64.pow(attempt))).await;
        }
    }

    warn!(merchant = %merchant_id, url, "Webhook failed after 3 attempts");
}