use axum::{
    routing::{get, post},
    Router, 
    Json, 
    response::{Html, IntoResponse}, 
    extract::State,
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
    Json(json!({
        "total_transactions": app_state.total_transactions,
        "balances": app_state.balances,
        "history": app_state.history,
        "operator": state.operator_address
    }))
}

async fn wallet_ui() -> impl IntoResponse {
    const HTML: &str = include_str!("../frontend/index.html");
    Html(HTML)
}