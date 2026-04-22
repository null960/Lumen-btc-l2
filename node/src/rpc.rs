use axum::{
    extract::{Extension, Path, State},
    http::{HeaderMap, StatusCode},
    middleware,
    response::Json,
    routing::{delete, get, post},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use chrono::Utc;
use tokio::sync::{mpsc, oneshot};
use tracing::info;

use crate::auth::{require_auth, MerchantId};
use crate::rate_limit::RateLimiter;
use crate::state::{AppState, Invoice, InvoiceStatus, Merchant};
use crate::webhook::WebhookManager;
use crate::NetworkEvent;

// ── Server state ──────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct ServerState {
    pub app:     Arc<Mutex<AppState>>,
    pub tx:      mpsc::Sender<NetworkEvent>,
    pub limiter: Arc<RateLimiter>,
    pub webhook: Arc<WebhookManager>,
}

// ── Request types ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct RegisterReq {
    pub name:  String,
    pub email: String,
}

#[derive(Deserialize)]
pub struct CreateInvoiceReq {
    pub amount_sats:  u64,
    pub description:  String,
    pub metadata:     Option<Value>,
    #[serde(default = "default_ttl")]
    pub expires_in:   u64,
}
fn default_ttl() -> u64 { 3600 }

#[derive(Deserialize)]
pub struct WebhookReq {
    pub url:    String,
    pub secret: Option<String>,
}

// ── Legacy request types (kept for Gemini's existing routes) ──────────────────

#[derive(Deserialize)]
pub struct TransferReq { pub to: String, pub amount: u64 }
#[derive(Deserialize)]
pub struct MintReq     { pub amount: u64 }
#[derive(Deserialize)]
pub struct InvoiceReq  { pub amount_sats: u64, pub description: String }
#[derive(Deserialize)]
pub struct PayReq      { pub invoice: String }
#[derive(Deserialize)]
pub struct PeerReq     { pub node_id: String, pub address: String }
#[derive(Deserialize)]
pub struct ChannelReq  { pub node_id: String, pub address: String, pub amount_sats: u64, pub push_msat: u64 }

// ── Router ────────────────────────────────────────────────────────────────────

pub async fn start_server(
    app:     Arc<Mutex<AppState>>,
    tx:      mpsc::Sender<NetworkEvent>,
    limiter: Arc<RateLimiter>,
) {
    let webhook = Arc::new(WebhookManager::new());
    let state   = ServerState { app: app.clone(), tx, limiter, webhook };

    // ── Public routes (no auth) ───────────────────────────────────────────────
    let public = Router::new()
        .route("/api/v1/register",   post(register))
        .route("/api/v1/health",     get(health))
        // Legacy node info (kept for Gemini compatibility)
        .route("/api/v1/info",       get(get_info))
        .route("/api/v1/address",    get(get_address))
        .route("/api/v1/channels",   get(get_channels));

    // ── Merchant routes (require API key) ─────────────────────────────────────
    let merchant = Router::new()
        // Account
        .route("/api/v1/account",           get(get_account))
        // Invoices
        .route("/api/v1/invoices",          post(create_invoice))
        .route("/api/v1/invoices",          get(list_invoices))
        .route("/api/v1/invoices/:id",      get(get_invoice))
        // Payments
        .route("/api/v1/payments",          get(list_payments))
        // Webhook config
        .route("/api/v1/webhook",           post(set_webhook))
        .route("/api/v1/webhook",           delete(delete_webhook))
        // Balance (new JSON version)
        .route("/api/v1/balance",           get(get_balance))
        // Legacy routes kept for backward compat
        .route("/api/v1/transfer",          post(transfer))
        .route("/api/v1/pay",               post(pay_invoice))
        .route("/api/v1/peer/connect",      post(connect_peer))
        .route("/api/v1/channel/open",      post(open_channel))
        .route_layer(middleware::from_fn_with_state(app, require_auth));

    // ── Dashboard ─────────────────────────────────────────────────────────────
    let dashboard = Router::new()
        .route("/", get(serve_dashboard));

    let app = public.merge(merchant).merge(dashboard).with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  ⚡ Lumen Bitcoin Payment API");
    println!("  🌐 http://0.0.0.0:3000");
    println!("  📖 POST /api/v1/register  — get started");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    axum::serve(listener, app).await.unwrap();
}

// ── Dashboard ─────────────────────────────────────────────────────────────────

async fn serve_dashboard() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("../frontend/index.html"))
}

// ── Health ────────────────────────────────────────────────────────────────────

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "version": "2.0.0" }))
}

// ── Registration ──────────────────────────────────────────────────────────────

async fn register(
    State(state): State<ServerState>,
    Json(req): Json<RegisterReq>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let name  = req.name.trim().to_string();
    let email = req.email.trim().to_lowercase();

    if name.is_empty()  { return Err(bad_request("name is required")); }
    if email.is_empty() { return Err(bad_request("email is required")); }
    if !email.contains('@') { return Err(bad_request("invalid email")); }

    let raw_key  = crate::auth::generate_key();
    let key_hash = crate::auth::hash_key(&raw_key);
    let key_hint = crate::auth::key_hint(&raw_key);
    let id       = uuid::Uuid::new_v4().to_string();

    {
        let mut s = state.app.lock().unwrap();

        if s.merchants.values().any(|m| m.email == email) {
            return Err((StatusCode::CONFLICT, Json(json!({
                "error":   "email_exists",
                "message": "This email is already registered"
            }))));
        }

        s.merchants.insert(id.clone(), Merchant {
            id:           id.clone(),
            name:         name.clone(),
            email:        email.clone(),
            api_key_hash: key_hash.clone(),
            api_key_hint: key_hint,
            balance_sats: 0,
            created_at:   Utc::now(),
            webhook_url:  None,
            webhook_secret: None,
            total_received_sats:  0,
            total_withdrawn_sats: 0,
        });
        s.key_index.insert(key_hash, id.clone());
    }

    info!(merchant_id = %id, email = %email, "Merchant registered");

    Ok((StatusCode::CREATED, Json(json!({
        "merchant_id": id,
        "api_key":     raw_key,
        "message":     "Save your API key — it will not be shown again.",
    }))))
}

// ── Account ───────────────────────────────────────────────────────────────────

async fn get_account(
    State(state): State<ServerState>,
    Extension(auth): Extension<MerchantId>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let s = state.app.lock().unwrap();
    let m = s.merchants.get(&auth.0).ok_or_else(|| not_found("Merchant not found"))?;

    Ok(Json(json!({
        "id":                    m.id,
        "name":                  m.name,
        "email":                 m.email,
        "api_key_hint":          m.api_key_hint,
        "balance_sats":          m.balance_sats,
        "balance_btc":           format!("{:.8}", m.balance_sats as f64 / 1e8),
        "total_received_sats":   m.total_received_sats,
        "total_withdrawn_sats":  m.total_withdrawn_sats,
        "webhook_url":           m.webhook_url,
        "has_webhook_secret":    m.webhook_secret.is_some(),
        "created_at":            m.created_at,
    })))
}

// ── Invoices ──────────────────────────────────────────────────────────────────

async fn create_invoice(
    State(state): State<ServerState>,
    Extension(auth): Extension<MerchantId>,
    Json(req): Json<CreateInvoiceReq>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    if req.amount_sats == 0 { return Err(bad_request("amount_sats must be > 0")); }
    if req.description.trim().is_empty() { return Err(bad_request("description is required")); }

    let (reply_tx, reply_rx) = oneshot::channel();
    state.tx.send(NetworkEvent::InvoiceRequest {
        account:     auth.0.clone(),
        amount_sats: req.amount_sats,
        description: req.description.clone(),
        reply:       reply_tx,
    }).await.map_err(|_| internal("Event channel closed"))?;

    let (bolt11, payment_hash) = reply_rx.await
        .map_err(|_| internal("Lightning node timeout"))?
        .map_err(|e| (StatusCode::SERVICE_UNAVAILABLE, Json(json!({
            "error": "lightning_error", "message": e
        }))))?;

    let invoice_id = uuid::Uuid::new_v4().to_string();
    let now        = Utc::now();
    let expires_at = now + chrono::Duration::seconds(req.expires_in as i64);

    let invoice = Invoice {
        id:           invoice_id.clone(),
        merchant_id:  auth.0.clone(),
        amount_sats:  req.amount_sats,
        description:  req.description,
        bolt11:       bolt11.clone(),
        payment_hash: payment_hash.clone(),
        status:       InvoiceStatus::Pending,
        created_at:   now,
        expires_at,
        paid_at:      None,
        metadata:     req.metadata,
    };

    {
        let mut s = state.app.lock().unwrap();
        // Also register in legacy pending_invoices so existing payment handler works
        s.pending_invoices.insert(payment_hash.clone(), auth.0.clone());
        s.invoices.insert(payment_hash, invoice.clone());
    }

    Ok((StatusCode::CREATED, Json(json!({
        "id":           invoice.id,
        "bolt11":       bolt11,
        "amount_sats":  invoice.amount_sats,
        "description":  invoice.description,
        "status":       "pending",
        "expires_at":   invoice.expires_at,
        "metadata":     invoice.metadata,
    }))))
}

async fn list_invoices(
    State(state): State<ServerState>,
    Extension(auth): Extension<MerchantId>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let s       = state.app.lock().unwrap();
    let invoices: Vec<Value> = s.invoices_for(&auth.0)
        .iter()
        .map(|i| invoice_json(i))
        .collect();
    Ok(Json(json!({ "invoices": invoices, "total": invoices.len() })))
}

async fn get_invoice(
    State(state): State<ServerState>,
    Extension(auth): Extension<MerchantId>,
    Path(invoice_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let s = state.app.lock().unwrap();
    let inv = s.invoices.values()
        .find(|i| i.id == invoice_id && i.merchant_id == auth.0)
        .ok_or_else(|| not_found("Invoice not found"))?;
    Ok(Json(invoice_json(inv)))
}

// ── Payments ──────────────────────────────────────────────────────────────────

async fn list_payments(
    State(state): State<ServerState>,
    Extension(auth): Extension<MerchantId>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let s = state.app.lock().unwrap();
    let payments: Vec<Value> = s.payments_for(&auth.0)
        .iter()
        .map(|p| json!({
            "id":           p.id,
            "invoice_id":   p.invoice_id,
            "amount_sats":  p.amount_sats,
            "payment_hash": p.payment_hash,
            "created_at":   p.created_at,
        }))
        .collect();
    Ok(Json(json!({ "payments": payments, "total": payments.len() })))
}

// ── Webhook config ────────────────────────────────────────────────────────────

async fn set_webhook(
    State(state): State<ServerState>,
    Extension(auth): Extension<MerchantId>,
    Json(req): Json<WebhookReq>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !req.url.starts_with("http://") && !req.url.starts_with("https://") {
        return Err(bad_request("URL must start with http:// or https://"));
    }
    let mut s = state.app.lock().unwrap();
    if let Some(m) = s.merchants.get_mut(&auth.0) {
        m.webhook_url    = Some(req.url.clone());
        m.webhook_secret = req.secret;
    }
    Ok(Json(json!({
        "status":  "ok",
        "url":     req.url,
        "message": "Webhook configured. Verify payloads using the X-Lumen-Signature header.",
    })))
}

async fn delete_webhook(
    State(state): State<ServerState>,
    Extension(auth): Extension<MerchantId>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut s = state.app.lock().unwrap();
    if let Some(m) = s.merchants.get_mut(&auth.0) {
        m.webhook_url    = None;
        m.webhook_secret = None;
    }
    Ok(Json(json!({ "status": "ok" })))
}

// ── Balance (new merchant-aware version) ──────────────────────────────────────

async fn get_balance(
    State(state): State<ServerState>,
    Extension(auth): Extension<MerchantId>,
    headers: HeaderMap,
) -> Json<Value> {
    let s = state.app.lock().unwrap();

    // New merchant system
    if let Some(m) = s.merchants.get(&auth.0) {
        return Json(json!({
            "merchant_id":  m.id,
            "balance_sats": m.balance_sats,
            "balance_btc":  format!("{:.8}", m.balance_sats as f64 / 1e8),
        }));
    }

    // Legacy fallback
    let account = headers.get("x-api-key")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("anonymous");
    let bal = s.balances.get(account).copied().unwrap_or(0);
    Json(json!({ "account": account, "balance_sats": bal }))
}

// ── Legacy routes (kept for backward compatibility) ───────────────────────────

async fn get_info(State(state): State<ServerState>) -> Json<Value> {
    let (tx, rx) = oneshot::channel();
    let _ = state.tx.send(NetworkEvent::GetNodeInfo { reply: tx }).await;
    let info = rx.await.unwrap_or_else(|_| json!({}));
    Json(info)
}

async fn get_address(State(state): State<ServerState>) -> Json<Value> {
    let (tx, rx) = oneshot::channel();
    let _ = state.tx.send(NetworkEvent::GetNewAddress { reply: tx }).await;
    let addr = rx.await.unwrap_or_default();
    Json(json!({ "address": addr }))
}

async fn get_channels(State(state): State<ServerState>) -> Json<Value> {
    let (tx, rx) = oneshot::channel();
    let _ = state.tx.send(NetworkEvent::ListChannels { reply: tx }).await;
    let channels = rx.await.unwrap_or_default();
    Json(json!({ "channels": channels }))
}

async fn transfer(
    Extension(auth): Extension<MerchantId>,
    State(state): State<ServerState>,
    Json(req): Json<TransferReq>,
) -> Json<Value> {
    let _ = state.tx.send(NetworkEvent::TransferRequest {
        from: auth.0, to: req.to, amount: req.amount,
    }).await;
    Json(json!({ "status": "processing" }))
}

async fn pay_invoice(
    Extension(auth): Extension<MerchantId>,
    State(state): State<ServerState>,
    Json(req): Json<PayReq>,
) -> Json<Value> {
    let (tx, rx) = oneshot::channel();
    let _ = state.tx.send(NetworkEvent::PayInvoiceRequest {
        account: auth.0, invoice: req.invoice, reply: tx,
    }).await;
    match rx.await {
        Ok(res) => Json(json!({ "status": res })),
        Err(_)  => Json(json!({ "error": "internal" })),
    }
}

async fn connect_peer(
    State(state): State<ServerState>,
    Json(req): Json<PeerReq>,
) -> Json<Value> {
    let (tx, rx) = oneshot::channel();
    let _ = state.tx.send(NetworkEvent::ConnectPeer {
        node_id: req.node_id, address: req.address, reply: tx,
    }).await;
    match rx.await { Ok(true) => Json(json!({ "status": "connected" })), _ => Json(json!({ "error": "failed" })) }
}

async fn open_channel(
    State(state): State<ServerState>,
    Json(req): Json<ChannelReq>,
) -> Json<Value> {
    let (tx, rx) = oneshot::channel();
    let _ = state.tx.send(NetworkEvent::OpenChannel {
        node_id: req.node_id, address: req.address,
        channel_amount_sats: req.amount_sats,
        push_to_reserve_msat: req.push_msat,
        reply: tx,
    }).await;
    match rx.await { Ok(true) => Json(json!({ "status": "opening" })), _ => Json(json!({ "error": "failed" })) }
}

// ── Error helpers ─────────────────────────────────────────────────────────────

fn bad_request(msg: &str) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": "validation_error", "message": msg })))
}
fn not_found(msg: &str) -> (StatusCode, Json<Value>) {
    (StatusCode::NOT_FOUND, Json(json!({ "error": "not_found", "message": msg })))
}
fn internal(msg: &str) -> (StatusCode, Json<Value>) {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal_error", "message": msg })))
}
fn invoice_json(i: &Invoice) -> Value {
    json!({
        "id":          i.id,
        "amount_sats": i.amount_sats,
        "description": i.description,
        "bolt11":      i.bolt11,
        "status":      format!("{:?}", i.status).to_lowercase(),
        "created_at":  i.created_at,
        "expires_at":  i.expires_at,
        "paid_at":     i.paid_at,
        "metadata":    i.metadata,
    })
}