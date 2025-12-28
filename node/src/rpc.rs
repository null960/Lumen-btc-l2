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

    // Count lines as mock block height
    let block_height = log_content.lines().count();

    let html = format!(
        r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>Lumen L2 Explorer</title>
            <style>
                body {{ font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif; background: #0d1117; color: #c9d1d9; margin: 0; padding: 40px; }}
                .container {{ max-width: 900px; margin: auto; }}
                .header {{ display: flex; justify-content: space-between; align-items: center; border-bottom: 1px solid #30363d; padding-bottom: 20px; }}
                .stats {{ display: grid; grid-template-columns: 1fr 1fr; gap: 20px; margin: 20px 0; }}
                .stat-card {{ background: #161b22; padding: 20px; border-radius: 8px; border: 1px solid #30363d; }}
                .stat-value {{ font-size: 24px; color: #58a6ff; font-weight: bold; }}
                .log-box {{ background: #010409; padding: 20px; border-radius: 8px; border: 1px solid #30363d; overflow-x: auto; line-height: 1.6; }}
                .anchor-line {{ color: #7ee787; border-bottom: 1px solid #21262d; padding: 5px 0; font-family: 'Courier New', monospace; }}
                .status {{ font-size: 14px; color: #8b949e; margin-top: 20px; }}
                .tag {{ background: #238636; color: white; padding: 2px 8px; border-radius: 10px; font-size: 12px; }}
            </style>
        </head>
        <body>
            <div class="container">
                <div class="header">
                    <h1>Lumen L2 Explorer <span class="tag">Mainnet-Sim</span></h1>
                    <button onclick="location.reload()" style="background:#21262d; color:white; border:1px solid #30363d; padding:8px 16px; border-radius:6px; cursor:pointer;">Refresh</button>
                </div>
                
                <div class="stats">
                    <div class="stat-card">
                        <div>Current Block Height</div>
                        <div class="stat-value">#{}</div>
                    </div>
                    <div class="stat-card">
                        <div>Network DA Layer</div>
                        <div class="stat-value" style="color:#f7931a">Nubit (Bitcoin)</div>
                    </div>
                </div>

                <h3>Recent Anchored Batches</h3>
                <div class="log-box">
                    {}
                </div>

                <div class="status">
                    🟢 Node Status: Active | Protocol: SVM-L2 | Bridge: Trusted-MPC
                </div>
            </div>
        </body>
        </html>
        "#,
        block_height,
        log_content.lines().rev().map(|line| format!("<div class='anchor-line'>{}</div>", line)).collect::<Vec<String>>().join("")
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