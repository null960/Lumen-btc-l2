use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

pub type Lsat = u64;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Merchant {
    pub id:           String,
    pub name:         String,
    pub email:        String,
    pub api_key_hash: String,
    pub api_key_hint: String,
    pub balance_sats: Lsat,
    pub created_at:   DateTime<Utc>,
    pub webhook_url:  Option<String>,
    pub webhook_secret: Option<String>,
    pub total_received_sats:  Lsat,
    pub total_withdrawn_sats: Lsat,
}

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
    pub payment_hash: String,
    pub status:       InvoiceStatus,
    pub created_at:   DateTime<Utc>,
    pub expires_at:   DateTime<Utc>,
    pub paid_at:      Option<DateTime<Utc>>,
    pub metadata:     Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PaymentRecord {
    pub id:           String,
    pub merchant_id:  String,
    pub invoice_id:   String,
    pub amount_sats:  Lsat,
    pub payment_hash: String,
    pub created_at:   DateTime<Utc>,
}