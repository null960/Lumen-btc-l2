use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};
use ldk_node::Builder;
use ldk_node::Network;
use std::net::SocketAddr;
use std::str::FromStr;
use serde_json::json;
use tracing::info;

use crate::state::{AppState, InvoiceStatus, PaymentRecord};
use crate::webhook::{WebhookEvent, WebhookManager};

mod auth;
mod rpc;
mod state;
mod storage;
mod webhook;
mod rate_limit;

// ── Network events ────────────────────────────────────────────────────────────

pub enum NetworkEvent {
    // Existing events (kept for Gemini compatibility)
    PaymentReceived   { payment_hash: String, amount_msat: u64 },
    TransferRequest   { from: String, to: String, amount: u64 },
    MintRequest       { account: String, amount: u64 },
    GetNodeInfo       { reply: oneshot::Sender<serde_json::Value> },
    GetNewAddress     { reply: oneshot::Sender<String> },
    ConnectPeer       { node_id: String, address: String, reply: oneshot::Sender<bool> },
    OpenChannel       { node_id: String, address: String, channel_amount_sats: u64, push_to_reserve_msat: u64, reply: oneshot::Sender<bool> },
    ListChannels      { reply: oneshot::Sender<Vec<serde_json::Value>> },
    PayInvoiceRequest { account: String, invoice: String, reply: oneshot::Sender<String> },

    // New events for merchant invoice creation
    InvoiceRequest {
        account:     String,
        amount_sats: u64,
        description: String,
        reply:       oneshot::Sender<Result<(String, String), String>>,
    },
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let storage       = Arc::new(storage::Storage::new("lumen_db.redb"));
    let initial_state = storage.load_state().unwrap_or_else(AppState::new);
    let app_state     = Arc::new(Mutex::new(initial_state));
    let webhook       = Arc::new(WebhookManager::new());
    let limiter       = Arc::new(rate_limit::RateLimiter::new());

    {
        let s = app_state.lock().unwrap();
        info!(
            merchants = s.merchants.len(),
            invoices  = s.invoices.len(),
            "State loaded"
        );
    }

    // ── Lightning node ────────────────────────────────────────────────────────
    let network = match std::env::var("BITCOIN_NETWORK")
        .unwrap_or_else(|_| "signet".into())
        .as_str()
    {
        "mainnet" | "bitcoin" => Network::Bitcoin,
        "testnet"             => Network::Testnet,
        _                     => Network::Signet,
    };

    let esplora = std::env::var("ESPLORA_URL").unwrap_or_else(|_| match network {
        Network::Bitcoin => "https://blockstream.info/api".into(),
        Network::Testnet => "https://blockstream.info/testnet/api".into(),
        _                => "https://mutinynet.com/api".into(),
    });

    let mut builder = Builder::new();
    builder.set_network(network);
    builder.set_esplora_server(esplora);
    builder.set_storage_dir_path("./ldk_data_v2".to_string());
    let _ = builder.set_listening_addresses(
        vec!["0.0.0.0:9735".parse::<SocketAddr>().unwrap().into()]
    );

    let node = Arc::new(builder.build().unwrap());
    node.start().unwrap();
    info!(node_id = %node.node_id(), "Lightning node started");

    let (tx, mut rx) = mpsc::channel::<NetworkEvent>(1024);

    // ── Lightning event listener ──────────────────────────────────────────────
    {
        let node_ev    = node.clone();
        let tx_ev      = tx.clone();
        let webhook_ev = webhook.clone();
        let state_ev   = app_state.clone();
        let storage_ev = storage.clone();

        tokio::spawn(async move {
            loop {
                if let Some(event) = node_ev.next_event() {
                    if let ldk_node::Event::PaymentReceived { payment_hash, amount_msat } = event {
                        let hash_hex = hex::encode(payment_hash.0);
                        let sats     = amount_msat / 1000;

                        handle_payment(&state_ev, &storage_ev, &webhook_ev, &hash_hex, sats).await;

                        // Also forward to sequencer for any legacy handlers
                        let _ = tx_ev.send(NetworkEvent::PaymentReceived {
                            payment_hash: hash_hex,
                            amount_msat,
                        }).await;

                        node_ev.event_handled();
                    } else {
                        node_ev.event_handled();
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        });
    }

    // ── Event sequencer ───────────────────────────────────────────────────────
    {
        let node_seq    = node.clone();
        let state_seq   = app_state.clone();
        let storage_seq = storage.clone();

        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {

                    // ── Already handled above — skip ──────────────────────────
                    NetworkEvent::PaymentReceived { .. } => {}

                    // ── New merchant invoice creation ─────────────────────────
                    NetworkEvent::InvoiceRequest { account: _, amount_sats, description, reply } => {
                        let result = node_seq
                            .receive_payment(amount_sats * 1000, &description, 3600)
                            .map(|inv| {
                                let hash_bytes: &[u8] = inv.payment_hash().as_ref();
                                let hash = hex::encode(hash_bytes);
                                (inv.to_string(), hash)
                            })
                            .map_err(|e| format!("{:?}", e));
                        let _ = reply.send(result);
                    }

                    // ── Legacy handlers (unchanged from Gemini) ───────────────

                    NetworkEvent::TransferRequest { from, to, amount } => {
                        let mut s = state_seq.lock().unwrap();
                        let from_bal = s.balances.get(&from).copied().unwrap_or(0);
                        if from_bal >= amount {
                            s.balances.insert(from, from_bal - amount);
                            let to_bal = s.balances.get(&to).copied().unwrap_or(0);
                            s.balances.insert(to, to_bal + amount);
                            s.total_transactions += 1;
                            storage_seq.save_state(&s).ok();
                        }
                    }

                    NetworkEvent::MintRequest { account, amount } => {
                        let mut s = state_seq.lock().unwrap();
                        let cur = s.balances.get(&account).copied().unwrap_or(0);
                        s.balances.insert(account, cur + amount);
                        s.total_transactions += 1;
                        storage_seq.save_state(&s).ok();
                    }

                    NetworkEvent::InvoiceRequest { .. } => {} // handled above

                    NetworkEvent::PayInvoiceRequest { account, invoice, reply } => {
                        let mut s = state_seq.lock().unwrap();
                        let bal = s.balances.get(&account).copied().unwrap_or(0);
                        if let Ok(inv) = invoice.parse::<ldk_node::lightning_invoice::Bolt11Invoice>() {
                            let amount_sats = inv.amount_milli_satoshis().unwrap_or(0) / 1000;
                            if bal >= amount_sats {
                                if let Ok(hash) = node_seq.send_payment(&inv) {
                                    s.balances.insert(account, bal - amount_sats);
                                    s.total_transactions += 1;
                                    storage_seq.save_state(&s).ok();
                                    let _ = reply.send(hex::encode(hash.0));
                                } else { let _ = reply.send("routing_failed".into()); }
                            } else { let _ = reply.send("insufficient_funds".into()); }
                        } else { let _ = reply.send("invalid_invoice".into()); }
                    }

                    NetworkEvent::GetNodeInfo { reply } => {
                        let onchain = node_seq.total_onchain_balance_sats().unwrap_or(0);
                        let _ = reply.send(json!({
                            "node_id":              node_seq.node_id().to_string(),
                            "onchain_balance_sats": onchain,
                            "num_peers":            node_seq.list_peers().len(),
                            "num_channels":         node_seq.list_channels().len(),
                        }));
                    }

                    NetworkEvent::GetNewAddress { reply } => {
                        let addr = node_seq.new_onchain_address()
                            .map(|a| a.to_string())
                            .unwrap_or_else(|_| "error".into());
                        let _ = reply.send(addr);
                    }

                    NetworkEvent::ConnectPeer { node_id, address, reply } => {
                        let ok = ldk_node::bitcoin::secp256k1::PublicKey::from_str(&node_id)
                            .ok()
                            .and_then(|pk| address.parse::<SocketAddr>().ok().map(|a| (pk, a)))
                            .map(|(pk, addr)| node_seq.connect(pk, addr.into(), true).is_ok())
                            .unwrap_or(false);
                        let _ = reply.send(ok);
                    }

                    NetworkEvent::OpenChannel { node_id, address, channel_amount_sats, push_to_reserve_msat, reply } => {
                        let ok = ldk_node::bitcoin::secp256k1::PublicKey::from_str(&node_id)
                            .ok()
                            .and_then(|pk| address.parse::<SocketAddr>().ok().map(|a| (pk, a)))
                            .map(|(pk, addr)| node_seq.connect_open_channel(
                                pk, addr.into(), channel_amount_sats,
                                Some(push_to_reserve_msat), None, true,
                            ).is_ok())
                            .unwrap_or(false);
                        let _ = reply.send(ok);
                    }

                    NetworkEvent::ListChannels { reply } => {
                        let channels = node_seq.list_channels().iter().map(|c| json!({
                            "channel_id":  hex::encode(c.channel_id.0),
                            "is_usable":   c.is_usable,
                            "is_ready":    c.is_channel_ready,
                            "value_sats":  c.channel_value_sats,
                            "balance_msat": c.balance_msat,
                        })).collect();
                        let _ = reply.send(channels);
                    }
                }
            }
        });
    }

    // ── HTTP server ───────────────────────────────────────────────────────────
    let node_shutdown = node.clone();
    tokio::select! {
        _ = rpc::start_server(app_state.clone(), tx, limiter.clone()) => {}
        _ = tokio::signal::ctrl_c() => {
            info!("Shutting down...");
            node_shutdown.stop().unwrap();
            storage.save_state(&app_state.lock().unwrap()).ok();
            info!("State saved. Goodbye.");
        }
    }
}

// ── Payment received — credits merchant balance and fires webhook ──────────────

async fn handle_payment(
    app_state: &Arc<Mutex<AppState>>,
    storage:   &Arc<storage::Storage>,
    webhook:   &Arc<WebhookManager>,
    hash:      &str,
    sats:      u64,
) {
    let webhook_fire = {
        let mut s = app_state.lock().unwrap();

        // Find invoice by payment hash
        let invoice = match s.invoices.get_mut(hash) {
            Some(i) => i,
            None    => {
                // Legacy path: credit account from pending_invoices
                if let Some(account) = s.pending_invoices.remove(hash) {
                    let cur = s.balances.get(&account).copied().unwrap_or(0);
                    s.balances.insert(account, cur + sats);
                    s.total_transactions += 1;
                    storage.save_state(&s).ok();
                }
                return;
            }
        };

        // Mark invoice paid
        invoice.status  = InvoiceStatus::Paid;
        invoice.paid_at = Some(chrono::Utc::now());

        let merchant_id = invoice.merchant_id.clone();
        let invoice_id  = invoice.id.clone();

        // Credit merchant
        if let Some(m) = s.merchants.get_mut(&merchant_id) {
            m.balance_sats          = m.balance_sats.saturating_add(sats);
            m.total_received_sats   = m.total_received_sats.saturating_add(sats);
        }

        // Record payment
        let record = PaymentRecord {
            id:           uuid::Uuid::new_v4().to_string(),
            merchant_id:  merchant_id.clone(),
            invoice_id:   invoice_id.clone(),
            amount_sats:  sats,
            payment_hash: hash.to_string(),
            created_at:   chrono::Utc::now(),
        };
        s.payments.insert(record.id.clone(), record);
        s.total_transactions  += 1;
        s.total_volume_sats   = s.total_volume_sats.saturating_add(sats);

        storage.save_state(&s).ok();

        info!(merchant = %merchant_id, sats, hash, "Payment credited");

        // Collect webhook info (must clone before dropping lock)
        s.merchants.get(&merchant_id).and_then(|m| {
            m.webhook_url.as_ref().map(|url| (
                merchant_id.clone(),
                url.clone(),
                m.webhook_secret.clone(),
                invoice_id,
            ))
        })
    };

    // Fire webhook outside the lock
    if let Some((merchant_id, url, secret, invoice_id)) = webhook_fire {
        webhook.fire(WebhookEvent {
            merchant_id: &merchant_id,
            url:         &url,
            secret:      secret.as_deref(),
            event:       "payment.received",
            data: json!({
                "invoice_id":   invoice_id,
                "amount_sats":  sats,
                "payment_hash": hash,
                "paid_at":      chrono::Utc::now().to_rfc3339(),
            }),
        });
    }
}