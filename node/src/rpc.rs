use axum::{
    routing::{get, post, delete},
    Router, Json,
    response::{Html, IntoResponse, Response},
    extract::{State, Path, ConnectInfo},
    http::{StatusCode, HeaderMap},
};
use std::sync::{Arc, Mutex};
use std::net::SocketAddr;
use tokio::sync::mpsc;
use crate::state::AppState;
use crate::NetworkEvent;
use crate::rate_limit::Limiters;
use crate::webhook::{WebhookManager, WebhookConfig};
use serde::Deserialize;
use serde_json::json;

// ── Server State ──────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct ServerState {
    pub app_state: Arc<Mutex<AppState>>,
    pub tx_channel: mpsc::Sender<NetworkEvent>,
    pub operator_address: String,
    pub limiters: Arc<Limiters>,
    pub webhooks: Arc<WebhookManager>,
}

// ── Request types ─────────────────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
pub struct UserCommand {
    pub cmd: String,
    pub sig: Option<String>,
    pub pubkey: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct TransferRequest {
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub memo: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct FaucetRequest {
    pub address: String,
}

#[derive(Deserialize, Debug)]
pub struct WebhookRegisterRequest {
    pub app_id: String,
    pub url: String,
    pub secret: Option<String>,
    pub events: Option<Vec<String>>,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Extract real IP from headers (handles reverse proxies)
fn extract_ip(headers: &HeaderMap, addr: &SocketAddr) -> String {
    if let Some(fwd) = headers.get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
    {
        return fwd.trim().to_string();
    }
    if let Some(real) = headers.get("x-real-ip")
        .and_then(|v| v.to_str().ok())
    {
        return real.trim().to_string();
    }
    addr.ip().to_string()
}

fn rate_limited_response(retry_after: i64) -> Response {
    let body = json!({
        "status": "error",
        "msg": "Rate limit exceeded",
        "retry_after_seconds": retry_after,
    });
    (
        StatusCode::TOO_MANY_REQUESTS,
        [("Retry-After", retry_after.to_string())],
        Json(body),
    ).into_response()
}

// ── Server Setup ──────────────────────────────────────────────────────────────

pub async fn run_server(
    state: Arc<Mutex<AppState>>,
    tx_channel: mpsc::Sender<NetworkEvent>,
    address: String,
    webhooks: Arc<WebhookManager>,
) {
    let limiters = Arc::new(Limiters::new());

    // Periodic cleanup of old rate limit entries
    let lim_cleanup = limiters.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(300)).await;
            lim_cleanup.write.cleanup();
            lim_cleanup.read.cleanup();
            lim_cleanup.faucet.cleanup();
        }
    });

    let shared_state = ServerState {
        app_state: state,
        tx_channel,
        operator_address: address,
        limiters,
        webhooks,
    };

    let app = Router::new()
        // Dashboard
        .route("/", get(wallet_ui))
        .route("/status", get(get_status))
        // Legacy endpoints
        .route("/faucet", post(request_faucet))
        .route("/balance/:address", get(get_user_balance_text))
        .route("/transfer", post(request_transfer))
        // Core API
        .route("/api/state", get(get_state))
        .route("/api/cmd", post(submit_command))
        .route("/api/balance/:address", get(get_balance_json))
        .route("/api/proof/:address", get(get_proof))
        .route("/api/settlements", get(get_settlements))
        .route("/api/apps", get(get_apps))
        .route("/api/apps/:app_id", get(get_app))
        .route("/api/bond", get(get_bond_status))
        .route("/api/withdrawals", get(get_withdrawals))
        // Recovery
        .route("/api/recovery/status", get(get_recovery_status))
        .route("/api/recovery/export", get(export_balances))
        // Webhooks
        .route("/api/webhooks", get(list_webhooks))
        .route("/api/webhooks/register", post(register_webhook))
        .route("/api/webhooks/:app_id", delete(unregister_webhook))
        // Rate limit stats (operator only in future)
        .route("/api/admin/rate-stats", get(get_rate_stats))
        .with_state(shared_state)
        .into_make_service_with_connect_info::<SocketAddr>();

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("🌍 Lumen Node RPC: http://0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}

// ── Status ────────────────────────────────────────────────────────────────────

async fn get_status() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "network": "Lumen-Testnet",
        "version": "2.0",
        "token": "LSAT"
    }))
}

// ── State ─────────────────────────────────────────────────────────────────────

async fn get_state(
    State(state): State<ServerState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    let ip = extract_ip(&headers, &addr);
    if !state.limiters.read.check(&ip) {
        return rate_limited_response(state.limiters.read.retry_after(&ip));
    }
    let app_state = state.app_state.lock().unwrap();
    Json(app_state.to_api_json(&state.operator_address)).into_response()
}

// ── Balance ───────────────────────────────────────────────────────────────────

async fn get_balance_json(
    State(state): State<ServerState>,
    Path(address): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    let ip = extract_ip(&headers, &addr);
    if !state.limiters.read.check(&ip) {
        return rate_limited_response(state.limiters.read.retry_after(&ip));
    }
    let app_state = state.app_state.lock().unwrap();
    let lsat = app_state.get_balance(&address);

    let mut app_tokens: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    for (key, &amount) in &app_state.app_token_balances {
        let parts: Vec<&str> = key.splitn(3, ':').collect();
        if parts.len() == 3 && parts[2] == address {
            app_tokens.insert(format!("{}:{}", parts[0], parts[1]), amount);
        }
    }

    Json(json!({
        "address": address,
        "lsat": lsat,
        "btc_equivalent": format!("{:.8} BTC", lsat as f64 / 100_000_000.0),
        "app_tokens": app_tokens,
    })).into_response()
}

async fn get_user_balance_text(
    State(state): State<ServerState>,
    Path(address): Path<String>,
) -> String {
    let app_state = state.app_state.lock().unwrap();
    format!("{} LSAT", app_state.get_balance(&address))
}

// ── Faucet ────────────────────────────────────────────────────────────────────

async fn request_faucet(
    State(state): State<ServerState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<FaucetRequest>,
) -> Response {
    let ip = extract_ip(&headers, &addr);

    // Two-layer rate limit: general write limit + strict faucet limit
    if !state.limiters.write.check(&ip) {
        return rate_limited_response(state.limiters.write.retry_after(&ip));
    }
    if !state.limiters.faucet.check(&ip) {
        let retry = state.limiters.faucet.retry_after(&ip);
        let mins = retry / 60;
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({
                "status": "error",
                "msg": format!("Faucet limit: 3 requests/hour. Try again in {} minutes.", mins),
                "retry_after_seconds": retry,
            }))
        ).into_response();
    }

    let address = payload.address.trim().to_string();
    if address.is_empty() {
        return (StatusCode::BAD_REQUEST,
            Json(json!({ "status": "error", "msg": "Address required" }))
        ).into_response();
    }

    // Anti-abuse: don't give faucet to rich addresses (> 100k LSAT)
    {
        let app_state = state.app_state.lock().unwrap();
        let bal = app_state.get_balance(&address);
        if bal > 100_000 {
            return (StatusCode::BAD_REQUEST,
                Json(json!({
                    "status": "error",
                    "msg": format!("Address already has {} LSAT. Faucet is for new users.", bal)
                }))
            ).into_response();
        }
    }

    let event = NetworkEvent::SystemCommand {
        address: address.clone(),
        cmd: "Faucet".to_string(),
    };
    match state.tx_channel.send(event).await {
        Ok(_) => Json(json!({
            "status": "success",
            "address": address,
            "amount": crate::state::FAUCET_AMOUNT,
            "msg": format!("{} LSAT sent to {}", crate::state::FAUCET_AMOUNT, address)
        })).into_response(),
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "status": "error", "msg": "Node busy" }))
        ).into_response(),
    }
}

// ── Transfer (legacy / CLI) ───────────────────────────────────────────────────

async fn request_transfer(
    State(state): State<ServerState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<TransferRequest>,
) -> Response {
    let ip = extract_ip(&headers, &addr);
    if !state.limiters.write.check(&ip) {
        return rate_limited_response(state.limiters.write.retry_after(&ip));
    }

    let memo_part = payload.memo.as_deref().unwrap_or("").trim().to_string();
    let cmd = if memo_part.is_empty() {
        format!("Transfer {} {}", payload.amount, payload.to)
    } else {
        format!("Transfer {} {} {}", payload.amount, payload.to, memo_part)
    };

    // Fire webhook for receiving app (if registered)
    let webhooks = state.webhooks.clone();
    let from_clone = payload.from.clone();
    let to_clone = payload.to.clone();
    let memo_clone = payload.memo.clone();
    let amount = payload.amount;

    let event = NetworkEvent::SystemCommand {
        address: payload.from.clone(),
        cmd,
    };
    match state.tx_channel.send(event).await {
        Ok(_) => {
            // Check if recipient has a webhook registered
            // We use address as app_id for direct payment webhooks
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                let txid = format!("L2_{}", chrono::Utc::now().timestamp_millis());
                webhooks.on_payment(&to_clone, &from_clone, &to_clone, amount, memo_clone, &txid);
            });
            Json(json!({ "status": "ok", "msg": "Transfer submitted" })).into_response()
        },
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "status": "error", "msg": "Node busy" }))
        ).into_response(),
    }
}

// ── Submit signed command ─────────────────────────────────────────────────────

async fn submit_command(
    State(state): State<ServerState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<UserCommand>,
) -> Response {
    let ip = extract_ip(&headers, &addr);
    if !state.limiters.write.check(&ip) {
        return rate_limited_response(state.limiters.write.retry_after(&ip));
    }

    match (&payload.sig, &payload.pubkey) {
        (Some(sig), Some(pubkey)) => {
            let secured = format!("SIGNED_CMD|{}|{}|{}", payload.cmd, sig, pubkey);
            let event = NetworkEvent::Transaction(secured);
            match state.tx_channel.send(event).await {
                Ok(_) => Json(json!({ "status": "ok", "msg": "Transaction queued" })).into_response(),
                Err(_) => (StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({ "status": "error", "msg": "Node overloaded" }))
                ).into_response(),
            }
        },
        _ => (StatusCode::BAD_REQUEST,
            Json(json!({ "status": "error", "msg": "Signature and pubkey required" }))
        ).into_response(),
    }
}

// ── Merkle Proof ──────────────────────────────────────────────────────────────

async fn get_proof(
    State(state): State<ServerState>,
    Path(address): Path<String>,
) -> Json<serde_json::Value> {
    let app_state = state.app_state.lock().unwrap();
    if let Some(proof) = crate::settlement::generate_merkle_proof(&app_state.balances, &address) {
        let bal = app_state.get_balance(&address);
        Json(json!({
            "status": "ok",
            "address": address,
            "balance_lsat": bal,
            "proof": proof,
            "state_root": app_state.latest_state_root,
        }))
    } else {
        Json(json!({ "status": "error", "msg": "Address not found or zero balance" }))
    }
}

// ── Settlements ───────────────────────────────────────────────────────────────

async fn get_settlements(State(state): State<ServerState>) -> Json<serde_json::Value> {
    let app_state = state.app_state.lock().unwrap();
    let settlements: Vec<&crate::state::TxRecord> = app_state.history.iter()
        .filter(|r| r.tx_type == "Settlement")
        .collect();
    Json(json!({ "settlements": settlements, "latest_root": app_state.latest_state_root }))
}

// ── Apps ──────────────────────────────────────────────────────────────────────

async fn get_apps(State(state): State<ServerState>) -> Json<serde_json::Value> {
    let app_state = state.app_state.lock().unwrap();
    let apps: Vec<serde_json::Value> = app_state.apps.values().map(|a| json!({
        "app_id": a.app_id, "app_name": a.app_name, "owner": a.owner,
        "token_name": a.token_name, "rate_per_lsat": a.rate_per_lsat,
        "description": a.description, "website": a.website,
        "lsat_collected": a.lsat_collected, "created_at": a.created_at,
    })).collect();
    Json(json!({ "apps": apps, "total": apps.len() }))
}

async fn get_app(
    State(state): State<ServerState>,
    Path(app_id): Path<String>,
) -> Json<serde_json::Value> {
    let app_state = state.app_state.lock().unwrap();
    match app_state.apps.get(&app_id) {
        Some(app) => Json(json!({
            "app_id": app.app_id, "app_name": app.app_name, "owner": app.owner,
            "token_name": app.token_name, "rate_per_lsat": app.rate_per_lsat,
            "description": app.description, "website": app.website,
            "lsat_collected": app.lsat_collected,
        })),
        None => Json(json!({ "status": "error", "msg": "App not found" })),
    }
}

// ── Bond & Withdrawals ────────────────────────────────────────────────────────

async fn get_bond_status(State(state): State<ServerState>) -> Json<serde_json::Value> {
    let app_state = state.app_state.lock().unwrap();
    let pending: u64 = app_state.withdrawals.values()
        .filter(|w| w.status == crate::state::WithdrawalStatus::Pending)
        .map(|w| w.amount).sum();
    Json(json!({
        "pending_withdrawals_lsat": pending,
        "min_bond_required_lsat": pending * 2,
        "challenge_window_hours": 24,
    }))
}

async fn get_withdrawals(State(state): State<ServerState>) -> Json<serde_json::Value> {
    let app_state = state.app_state.lock().unwrap();
    let withdrawals: Vec<serde_json::Value> = app_state.withdrawals.values().map(|w| json!({
        "id": w.id, "user": w.user, "btc_address": w.btc_address,
        "amount_lsat": w.amount, "status": format!("{:?}", w.status),
        "created_at": w.created_at, "challenge_deadline": w.challenge_deadline,
    })).collect();
    Json(json!({ "withdrawals": withdrawals }))
}

// ── Webhooks ──────────────────────────────────────────────────────────────────

async fn register_webhook(
    State(state): State<ServerState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<WebhookRegisterRequest>,
) -> Response {
    let ip = extract_ip(&headers, &addr);
    if !state.limiters.write.check(&ip) {
        return rate_limited_response(state.limiters.write.retry_after(&ip));
    }

    // Validate URL
    if !payload.url.starts_with("http://") && !payload.url.starts_with("https://") {
        return (StatusCode::BAD_REQUEST,
            Json(json!({ "status": "error", "msg": "URL must start with http:// or https://" }))
        ).into_response();
    }

    // Verify app exists
    {
        let app_state = state.app_state.lock().unwrap();
        if !app_state.apps.contains_key(&payload.app_id) {
            return (StatusCode::NOT_FOUND,
                Json(json!({ "status": "error", "msg": format!("App '{}' not found", payload.app_id) }))
            ).into_response();
        }
    }

    let config = WebhookConfig {
        app_id: payload.app_id.clone(),
        url: payload.url.clone(),
        secret: payload.secret,
        events: payload.events,
        created_at: chrono::Utc::now().timestamp(),
        delivered: 0,
        failed: 0,
        active: true,
    };

    state.webhooks.register(config);
    println!("🔔 Webhook registered: {} → {}", payload.app_id, payload.url);

    Json(json!({
        "status": "ok",
        "msg": format!("Webhook registered for app '{}'", payload.app_id),
        "app_id": payload.app_id,
        "url": payload.url,
        "note": "Sign payload with X-Lumen-Signature header for security"
    })).into_response()
}

async fn unregister_webhook(
    State(state): State<ServerState>,
    Path(app_id): Path<String>,
) -> Json<serde_json::Value> {
    state.webhooks.unregister(&app_id);
    Json(json!({ "status": "ok", "msg": format!("Webhook removed for '{}'", app_id) }))
}

async fn list_webhooks(State(state): State<ServerState>) -> Json<serde_json::Value> {
    let webhooks = state.webhooks.list();
    let safe: Vec<serde_json::Value> = webhooks.iter().map(|w| json!({
        "app_id": w.app_id,
        "url": w.url,
        "active": w.active,
        "delivered": w.delivered,
        "failed": w.failed,
        "created_at": w.created_at,
        "has_secret": w.secret.is_some(),
        "events": w.events,
    })).collect();
    Json(json!({ "webhooks": safe, "total": safe.len() }))
}

// ── Rate limit stats ──────────────────────────────────────────────────────────

async fn get_rate_stats(State(state): State<ServerState>) -> Json<serde_json::Value> {
    let (write_ips, write_blocked) = state.limiters.write.stats();
    let (read_ips, read_blocked) = state.limiters.read.stats();
    let (faucet_ips, faucet_blocked) = state.limiters.faucet.stats();
    Json(json!({
        "write": { "active_ips": write_ips, "total_blocked": write_blocked, "limit": "20/min" },
        "read":  { "active_ips": read_ips,  "total_blocked": read_blocked,  "limit": "120/min" },
        "faucet":{ "active_ips": faucet_ips,"total_blocked": faucet_blocked,"limit": "3/hour" },
    }))
}

// ── Recovery ──────────────────────────────────────────────────────────────────

async fn get_recovery_status(State(state): State<ServerState>) -> Json<serde_json::Value> {
    let app_state = state.app_state.lock().unwrap();
    let da = crate::da_adapter::BitcoinDAAdapter::new("lumen_da");
    let batch_count = da.batch_count();
    let has_snapshot = da.load_latest_snapshot().is_some();
    Json(json!({
        "da_batches": batch_count,
        "has_snapshot": has_snapshot,
        "current_state_root": app_state.latest_state_root,
        "total_transactions": app_state.total_transactions,
        "accounts": app_state.balances.len(),
        "recovery_possible": batch_count > 0 || has_snapshot,
        "note": "Restart node to auto-recover from DA batches",
        "manual_recovery": "DELETE lumen_db and restart",
    }))
}

async fn export_balances(State(state): State<ServerState>) -> String {
    let app_state = state.app_state.lock().unwrap();
    crate::recovery::export_balance_snapshot(&app_state)
}

// ── Dashboard ─────────────────────────────────────────────────────────────────

async fn wallet_ui() -> impl IntoResponse {
    const HTML: &str = include_str!("../frontend/index.html");
    Html(HTML)
}