use axum::{
    routing::{get, post},
    Router,
    Json,
    extract::State,
    http::StatusCode,
    response::Html,
};
use crate::mempool::{SharedMempool, L2Transaction};
use serde::Serialize;
use std::fs;

#[derive(Debug, Serialize)]
pub struct SubmitTransactionResponse {
    pub status: String,
    pub tx_id: String,
}

async fn explorer_handler() -> Html<String> {
    let log_content = fs::read_to_string("batches.log")
        .unwrap_or_else(|_| "No batches anchored yet.".to_string());

    // Basic HTML styling for the explorer
    let html = format!(
        "<html>
            <head><title>Lumen-btc-l2 Explorer</title></head>
            <body style='font-family: monospace; background: #1a1a1a; color: #00ff00; padding: 20px;'>
                <h1>🟠 Lumen-btc-l2 Block Explorer</h1>
                <hr>
                <h2>Recent Bitcoin Anchors (Data Availability)</h2>
                <pre style='background: #2a2a2a; padding: 15px; border-radius: 5px;'>{}</pre>
                <hr>
                <p>Status: Node Online | Network: Bitcoin Signet</p>
            </body>
        </html>",
        log_content
    );

    Html(html)
}

async fn submit_tx_handler(
    State(mempool): State<SharedMempool>,
    Json(payload): Json<L2Transaction>,
) -> (StatusCode, Json<SubmitTransactionResponse>) {
    
    if !payload.verify_signature() {
        return (StatusCode::BAD_REQUEST, Json(SubmitTransactionResponse {
            status: "Error: Invalid Signature".to_string(),
            tx_id: "none".to_string(),
        }));
    }

    let mut q = mempool.lock().unwrap();
    let mock_id = format!("lumen_tx_{}", q.len() + 1);
    q.push_back(payload.clone());

    (StatusCode::OK, Json(SubmitTransactionResponse {
        status: "Queued".to_string(),
        tx_id: mock_id,
    }))
}

pub async fn start_rpc_server(mempool: SharedMempool) {
    let app = Router::new()
        .route("/health", get(|| async { "Lumen L2 Node is Healthy! 🟢" }))
        .route("/submit", post(submit_tx_handler))
        .route("/explorer", get(explorer_handler))
        .with_state(mempool);

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("🌐 RPC Server listening on http://{}", addr);
    println!("📊 Block Explorer available at http://{}/explorer", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}