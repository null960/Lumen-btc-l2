// Imports
use axum::{
    routing::{get, post},
    Router, 
    Json, 
    response::{Html, IntoResponse}, 
    extract::{State, Path},
};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use crate::state::AppState;
use crate::NetworkEvent;
use serde::Deserialize;
use serde_json::json;

// State
#[derive(Clone)]
pub struct ServerState {
    pub app_state: Arc<Mutex<AppState>>,
    pub tx_channel: mpsc::Sender<NetworkEvent>,
    pub operator_address: String,
}

// Struct
#[derive(Deserialize, Debug)]
pub struct FaucetRequest {
    pub address: Option<String>,
}

// Struct
#[derive(Deserialize, Debug)]
pub struct UserCommand {
    pub cmd: String,
    pub sig: Option<String>,
    pub pubkey: Option<String>,
}

// Struct
#[derive(Deserialize, Debug)]
pub struct TransferRequest {
    pub from: String,
    pub to: String,
    pub amount: u64,
}

// Server
pub async fn run_server(
    state: Arc<Mutex<AppState>>, 
    tx_channel: mpsc::Sender<NetworkEvent>, 
    address: String
) {
    let shared_state = ServerState {
        app_state: state,
        tx_channel,
        operator_address: address,
    };

    let app = Router::new()
        .route("/", get(wallet_ui)) 
        .route("/status", get(get_status))
        .route("/faucet", post(request_faucet))
        .route("/balance/:address", get(get_user_balance))
        .route("/transfer", post(request_transfer))
        .route("/api/state", get(get_state))
        .route("/api/cmd", post(submit_command))
        .route("/api/proof/:address", get(get_proof))
        .with_state(shared_state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("🌍 Remote RPC Server running at http://0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}

// Status
async fn get_status() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok", "network": "Lumen-Alpha-Testnet" }))
}

// Faucet
async fn request_faucet(
    State(state): State<ServerState>,
    Json(payload): Json<FaucetRequest>,
) -> Json<serde_json::Value> {
    let target_addr = payload.address.unwrap_or_else(|| "LumenUser_New".to_string());
    
    let cmd = format!("Faucet {}", target_addr);
    let event = NetworkEvent::Transaction(format!("SIGNED_CMD|{}|faucet_sig|system", cmd));
    
    match state.tx_channel.send(event).await {
        Ok(_) => Json(json!({ 
            "status": "success", 
            "address": target_addr, 
            "amount": 1000,
            "msg": "Funds queued for delivery" 
        })),
        Err(_) => Json(json!({ "status": "error", "msg": "Node busy" })),
    }
}

// Balance
async fn get_user_balance(
    State(state): State<ServerState>,
    Path(address): Path<String>,
) -> String {
    let app_state = state.app_state.lock().unwrap();
    let bal = app_state.balances.get(&address)
        .and_then(|b| b.get("BTC"))
        .unwrap_or(&0);
    format!("{} BTC", bal)
}

// Transfer
async fn request_transfer(
    State(state): State<ServerState>,
    Json(payload): Json<TransferRequest>,
) -> Json<serde_json::Value> {
    let cmd = format!("Transfer {} {} BTC", payload.amount, payload.to);
    let event = NetworkEvent::Transaction(format!("SIGNED_CMD|{}|cli_test_sig|{}", cmd, payload.from));
    match state.tx_channel.send(event).await {
        Ok(_) => Json(json!({ "status": "success", "msg": "Transfer submitted" })),
        Err(_) => Json(json!({ "status": "error", "msg": "Node busy" })),
    }
}

// Command
async fn submit_command(
    State(state): State<ServerState>,
    Json(payload): Json<UserCommand>,
) -> Json<serde_json::Value> {
    if let Some(signature) = &payload.sig {
        let secured_cmd = format!(
            "SIGNED_CMD|{}|{}|{}", 
            payload.cmd, 
            signature, 
            payload.pubkey.as_deref().unwrap_or("")
        );
        let event = NetworkEvent::Transaction(secured_cmd);
        match state.tx_channel.send(event).await {
            Ok(_) => Json(json!({ "status": "ok", "msg": "Transaction queued" })),
            Err(_) => Json(json!({ "status": "error", "msg": "Node is overloaded" })),
        }
    } else {
        Json(json!({ "status": "error", "msg": "Signature required" }))
    }
}

// State
async fn get_state(State(state): State<ServerState>) -> Json<serde_json::Value> {
    let app_state = state.app_state.lock().unwrap();
    
    let tokens_info: Vec<serde_json::Value> = app_state.tokens.iter().map(|(ticker, meta)| {
        json!({
            "ticker": ticker,
            "name": meta.name,
            "supply": meta.supply,
            "issuer": meta.issuer,
            "description": meta.description
        })
    }).collect();

    Json(json!({
        "total_transactions": app_state.total_transactions,
        "balances": app_state.balances,
        "tokens": tokens_info,
        "history": app_state.history,
        "operator": state.operator_address,
        "latest_state_root": app_state.latest_state_root 
    }))
}

// Proof
async fn get_proof(
    State(state): State<ServerState>,
    Path(address): Path<String>,
) -> Json<serde_json::Value> {
    let app_state = state.app_state.lock().unwrap();
    
    let mut btc_balances = std::collections::HashMap::new();
    for (user, bals) in &app_state.balances {
        if let Some(amt) = bals.get("BTC") {
            btc_balances.insert(user.clone(), *amt);
        }
    }

    if let Some(proof) = crate::settlement::generate_merkle_proof(&btc_balances, &address) {
        let bal = btc_balances.get(&address).unwrap_or(&0);
        Json(json!({ 
            "status": "ok", 
            "address": address, 
            "balance": bal, 
            "proof": proof, 
            "root": app_state.latest_state_root 
        }))
    } else {
        Json(json!({ "status": "error", "msg": "Address not found or no BTC balance" }))
    }
}

// UI
async fn wallet_ui() -> impl IntoResponse {
    const HTML: &str = include_str!("../frontend/index.html");
    Html(HTML)
}