use axum::{
    routing::{get, post},
    Router,
    Json,
    extract::State,
    http::StatusCode,
};
use crate::mempool::{SharedMempool, L2Transaction};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SubmitTransactionResponse {
    pub status: String,
    pub tx_id: String,
}

async fn submit_tx_handler(
    State(mempool): State<SharedMempool>,
    Json(payload): Json<L2Transaction>,
) -> (StatusCode, Json<SubmitTransactionResponse>) {
    
    // Check if the transaction is authentic (SVM Security)
    if !payload.verify_signature() {
        println!("❌ RPC: Unauthorized transaction attempt from {}", payload.sender);
        return (StatusCode::BAD_REQUEST, Json(SubmitTransactionResponse {
            status: "Error: Invalid Signature".to_string(),
            tx_id: "none".to_string(),
        }));
    }

    let mut q = mempool.lock().unwrap();
    let mock_id = format!("lumen_tx_{}", q.len() + 1);
    
    q.push_back(payload.clone());
    println!("📥 RPC: Verified & Accepted Tx from {}", payload.sender);

    (StatusCode::OK, Json(SubmitTransactionResponse {
        status: "Queued for Bitcoin DA".to_string(),
        tx_id: mock_id,
    }))
}

pub async fn start_rpc_server(mempool: SharedMempool) {
    let app = Router::new()
        .route("/health", get(|| async { "Lumen L2 Node is Healthy! 🟢" }))
        .route("/submit", post(submit_tx_handler))
        .with_state(mempool);

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("🌐 RPC Server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}