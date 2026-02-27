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

#[derive(Clone)]
pub struct ServerState {
    pub app_state: Arc<Mutex<AppState>>,
    pub tx_channel: mpsc::Sender<NetworkEvent>,
    pub operator_address: String,
}

#[derive(Deserialize, Debug)]
pub struct UserCommand {
    pub cmd: String,
    pub sig: Option<String>,
    pub pubkey: Option<String>,
}

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
        .route("/api/state", get(get_state))
        .route("/api/cmd", post(submit_command))
        .route("/api/proof/:address", get(get_proof))
        .with_state(shared_state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("🌍 Local Server running at http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
}

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

async fn wallet_ui() -> impl IntoResponse {
    const HTML: &str = include_str!("../frontend/index.html");
    Html(HTML)
}