use axum::{
    routing::{get, post},
    Router,
    Json,
    extract::State,
    http::StatusCode,
    response::Html,
};
use crate::mempool::{SharedMempool, L2Transaction};
use crate::state::SharedState;
use serde::Serialize;
use std::fs;

#[derive(Clone)]
pub struct RpcState {
    pub mempool: SharedMempool,
    pub state: SharedState,
}

#[derive(Debug, Serialize)]
pub struct SubmitTransactionResponse {
    pub status: String,
    pub tx_id: String,
}

async fn explorer_handler(State(rpc_state): State<RpcState>) -> Html<String> {
    let log_content = fs::read_to_string("batches.log")
        .unwrap_or_else(|_| "No batches anchored yet.".to_string());

    let state = rpc_state.state.lock().unwrap();
    let total_txs = state.total_transactions;
    let contract_counter = state.smart_contract_counter;
    
    let block_height = log_content.lines().count();

    let html = format!(
        r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>Lumen L2 Explorer</title>
            <meta http-equiv="refresh" content="5"> <style>
                body {{ font-family: 'Segoe UI', sans-serif; background: #0d1117; color: #c9d1d9; padding: 40px; }}
                .container {{ max-width: 900px; margin: auto; }}
                .header {{ border-bottom: 1px solid #30363d; padding-bottom: 20px; margin-bottom: 20px; }}
                .stats-grid {{ display: grid; grid-template-columns: repeat(3, 1fr); gap: 15px; margin-bottom: 30px; }}
                .card {{ background: #161b22; padding: 20px; border-radius: 8px; border: 1px solid #30363d; }}
                .card h3 {{ margin: 0 0 10px 0; font-size: 14px; color: #8b949e; }}
                .card .val {{ font-size: 28px; font-weight: bold; color: #58a6ff; }}
                .card .val.green {{ color: #3fb950; }}
                .card .val.orange {{ color: #f7931a; }}
                .log-box {{ background: #010409; padding: 15px; border-radius: 8px; border: 1px solid #30363d; font-family: monospace; color: #7ee787; }}
            </style>
        </head>
        <body>
            <div class="container">
                <div class="header">
                    <h1>🟠 Lumen-btc-l2 <span style="font-size:14px; background:#238636; padding:2px 8px; border-radius:10px;">Live Testnet</span></h1>
                </div>

                <div class="stats-grid">
                    <div class="card">
                        <h3>Smart Contract State</h3>
                        <div class="val green">{}</div> <small>Global Counter</small>
                    </div>
                    <div class="card">
                        <h3>Total Transactions</h3>
                        <div class="val">{}</div>
                    </div>
                    <div class="card">
                        <h3>Block Height</h3>
                        <div class="val orange">#{}</div>
                    </div>
                </div>

                <h3>📜 Data Availability Log (Bitcoin Anchors)</h3>
                <div class="log-box">
                    {}
                </div>
            </div>
        </body>
        </html>
        "#,
        contract_counter,
        total_txs,
        block_height,
        log_content.lines().rev().collect::<Vec<_>>().join("<br>")
    );

    Html(html)
}

async fn submit_tx_handler(
    State(rpc_state): State<RpcState>,
    Json(payload): Json<L2Transaction>,
) -> (StatusCode, Json<SubmitTransactionResponse>) {
    

    if !payload.verify_signature() {
         return (StatusCode::BAD_REQUEST, Json(SubmitTransactionResponse {
             status: "Invalid Signature".to_string(),
             tx_id: "none".to_string(),
         }));
    }

    let mut q = rpc_state.mempool.lock().unwrap();
    q.push_back(payload);

    (StatusCode::OK, Json(SubmitTransactionResponse {
        status: "Queued".to_string(),
        tx_id: "pending".to_string(),
    }))
}

pub async fn start_rpc_server(mempool: SharedMempool, state: SharedState) {
    let rpc_state = RpcState { mempool, state };

    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/submit", post(submit_tx_handler))
        .route("/explorer", get(explorer_handler))
        .with_state(rpc_state);

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("🌐 Explorer UI: http://{}/explorer", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}