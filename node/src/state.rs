use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};

pub type Lsat = u64;

// ── Merchant ──────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Merchant {
    pub id:           String,
    pub name:         String,
    pub email:        String,
    /// SHA-256 of raw API key — never store raw
    pub api_key_hash: String,
    /// "lm_live_ab12cd34..." — shown in dashboard for identification
    pub api_key_hint: String,
    pub balance_sats: Lsat,
    pub created_at:   DateTime<Utc>,
    /// URL to POST payment events to
    pub webhook_url:  Option<String>,
    /// HMAC-SHA256 secret for signing webhook payloads
    pub webhook_secret: Option<String>,
    pub total_received_sats:  Lsat,
    pub total_withdrawn_sats: Lsat,
}

// ── Invoice ───────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum InvoiceStatus {
    Pending,
    Paid,
    Expired,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Invoice {
    pub id:           String,
    pub merchant_id:  String,
    pub amount_sats:  Lsat,
    pub description:  String,
    pub bolt11:       String,
    /// Lightning payment hash — key used to match incoming payments
    pub payment_hash: String,
    pub status:       InvoiceStatus,
    pub created_at:   DateTime<Utc>,
    pub expires_at:   DateTime<Utc>,
    pub paid_at:      Option<DateTime<Utc>>,
    pub metadata:     Option<serde_json::Value>,
}

// ── Payment record ────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PaymentRecord {
    pub id:           String,
    pub merchant_id:  String,
    pub invoice_id:   String,
    pub amount_sats:  Lsat,
    pub payment_hash: String,
    pub created_at:   DateTime<Utc>,
}

// ── App state ─────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppState {
    // ── Existing fields (kept for backward compat with ldk_data) ─────────────
    pub balances:           HashMap<String, Lsat>,
    pub pending_invoices:   HashMap<String, String>,
    pub total_transactions: u64,

    // ── New merchant system ───────────────────────────────────────────────────
    /// merchant_id → Merchant
    pub merchants:     HashMap<String, Merchant>,
    /// api_key_hash → merchant_id  (fast lookup index)
    pub key_index:     HashMap<String, String>,
    /// payment_hash → Invoice
    pub invoices:      HashMap<String, Invoice>,
    /// payment_id → PaymentRecord
    pub payments:      HashMap<String, PaymentRecord>,

    pub total_volume_sats: Lsat,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            balances:           HashMap::new(),
            pending_invoices:   HashMap::new(),
            total_transactions: 0,
            merchants:          HashMap::new(),
            key_index:          HashMap::new(),
            invoices:           HashMap::new(),
            payments:           HashMap::new(),
            total_volume_sats:  0,
        }
    }

    // ── Merchant lookups ──────────────────────────────────────────────────────

    pub fn merchant_by_key(&self, raw_key: &str) -> Option<&Merchant> {
        let hash = crate::auth::hash_key(raw_key);
        let id   = self.key_index.get(&hash)?;
        self.merchants.get(id)
    }

    pub fn merchant_by_key_mut(&mut self, raw_key: &str) -> Option<&mut Merchant> {
        let hash = crate::auth::hash_key(raw_key);
        let id   = self.key_index.get(&hash)?.clone();
        self.merchants.get_mut(&id)
    }

    // ── Invoice queries ───────────────────────────────────────────────────────

    /// All invoices for a merchant, newest first, capped at 100
    pub fn invoices_for(&self, merchant_id: &str) -> Vec<&Invoice> {
        let mut list: Vec<&Invoice> = self.invoices.values()
            .filter(|i| i.merchant_id == merchant_id)
            .collect();
        list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        list.truncate(100);
        list
    }

    /// All payment records for a merchant, newest first, capped at 100
    pub fn payments_for(&self, merchant_id: &str) -> Vec<&PaymentRecord> {
        let mut list: Vec<&PaymentRecord> = self.payments.values()
            .filter(|p| p.merchant_id == merchant_id)
            .collect();
        list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        list.truncate(100);
        list
    }
}