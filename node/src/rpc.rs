use axum::{
    extract::{State, Json},
    response::Html,
    routing::{get, post},
    Router,
};
use std::sync::{Arc, Mutex};
use std::collections::VecDeque; 
use crate::state::AppState;
use crate::mempool::L2Transaction;
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};

// --- HTML INTERFACE ---
const HTML_PAGE: &str = r#"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <title>Lumen Terminal</title>
    <style>
        body { background: #0d1117; color: #c9d1d9; font-family: 'Consolas', monospace; display: flex; flex-direction: column; align-items: center; padding: 20px; }
        .container { width: 800px; max-width: 90%; }
        
        .header { text-align: center; margin-bottom: 20px; }
        .header h1 { color: #58a6ff; margin: 0; }
        .header p { color: #8b949e; font-size: 0.9em; }

        .dashboard { display: flex; gap: 20px; margin-bottom: 20px; }
        .card { background: #161b22; padding: 15px; border: 1px solid #30363d; border-radius: 6px; flex: 1; text-align: center; }
        .card h3 { margin: 0 0 10px 0; font-size: 0.8em; color: #8b949e; text-transform: uppercase; }
        .balance { font-size: 1.8em; font-weight: bold; color: #fff; }
        
        .terminal { background: #000; border: 1px solid #30363d; border-radius: 6px; padding: 15px; height: 300px; overflow-y: auto; font-size: 0.9em; box-shadow: 0 10px 30px rgba(0,0,0,0.5); }
        .line { margin-bottom: 5px; }
        .line.user { color: #58a6ff; }
        .line.system { color: #7ee787; }
        .line.error { color: #ff7b72; }

        .input-area { display: flex; margin-top: 10px; }
        .prompt { color: #58a6ff; padding: 10px 0 10px 10px; background: #161b22; border: 1px solid #30363d; border-right: none; border-radius: 6px 0 0 6px; font-weight: bold; }
        input { flex: 1; background: #161b22; border: 1px solid #30363d; border-left: none; color: #fff; padding: 10px; font-family: inherit; outline: none; border-radius: 0 6px 6px 0; }

        .help-box { margin-top: 20px; background: #161b22; padding: 15px; border-radius: 6px; border: 1px solid #30363d; font-size: 0.85em; }
        .cmd { color: #d2a8ff; font-weight: bold; }
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>Lumen-BTC Terminal</h1>
            <p>Phase 6</p>
        </div>

        <div class="dashboard">
            <div class="card">
                <h3>My L2 Address</h3>
                <div style="font-size: 0.8em; word-break: break-all;" id="myAddr">0x107cb97206f84fa3</div>
            </div>
            <div class="card">
                <h3>Balance (LBTC)</h3>
                <div class="balance" id="bal">0.00000000</div>
            </div>
        </div>

        <div class="terminal" id="term">
            <div class="line system">Welcome to Lumen Node v0.1.0</div>
            <div class="line system">Connected to Bitcoin Regtest.</div>
            <div class="line system">Type 'help' for available commands.</div>
        </div>

        <div class="input-area">
            <div class="prompt">></div>
            <input type="text" id="cmdInput" placeholder="Enter command..." autofocus>
        </div>

        <div class="help-box">
            <strong>Available Commands:</strong><br>
            <span class="cmd">Transfer &lt;amount_sats&gt; &lt;to_l2_addr&gt;</span> - Send LBTC to another user<br>
            <span class="cmd">Withdraw &lt;amount_sats&gt; &lt;btc_addr&gt;</span> - Burn LBTC and receive real BTC<br>
            <span class="cmd">Faucet &lt;btc_addr&gt;</span> - Get 0.1 test BTC from miner<br>
            <span class="cmd">refresh</span> - Force update balance data
        </div>
    </div>

    <script>
        const term = document.getElementById('term');
        const input = document.getElementById('cmdInput');
        const myAddr = document.getElementById('myAddr').innerText;

        function log(msg, type = 'system') {
            const div = document.createElement('div');
            div.className = 'line ' + type;
            div.innerText = msg;
            term.appendChild(div);
            term.scrollTop = term.scrollHeight;
        }

        async function fetchState() {
            try {
                const res = await fetch('/api/state');
                const data = await res.json();
                const sats = data.balances[myAddr] || 0;
                document.getElementById('bal').innerText = (sats / 100000000).toFixed(8);
            } catch(e) {}
        }

        async function sendCommand(cmd) {
            log('> ' + cmd, 'user');
            
            if (cmd === 'help') {
                log('Commands: Transfer, Withdraw, Faucet, refresh');
                return;
            }
            if (cmd === 'refresh') {
                fetchState();
                log('Data refreshed.');
                return;
            }

            try {
                const res = await fetch('/api/send', {
                    method: 'POST',
                    headers: {'Content-Type': 'application/json'},
                    body: JSON.stringify({
                        sender: myAddr,
                        instruction: cmd,
                        signature: "dummy_sig"
                    })
                });
                if (res.ok) {
                    log('Command sent to Mempool.', 'system');
                    setTimeout(fetchState, 4000);
                } else {
                    log('Error sending command', 'error');
                }
            } catch (e) {
                log('Network error: ' + e, 'error');
            }
        }

        input.addEventListener('keydown', (e) => {
            if (e.key === 'Enter') {
                const cmd = input.value.trim();
                if (cmd) sendCommand(cmd);
                input.value = '';
            }
        });

        fetchState();
        setInterval(fetchState, 5000);
    </script>
</body>
</html>
"#;

#[derive(Deserialize)]
struct TxRequest {
    sender: String,
    instruction: String,
    signature: String,
}

pub async fn start_rpc_server(
    mempool: Arc<Mutex<VecDeque<L2Transaction>>>, 
    state: Arc<Mutex<AppState>>,
) {
    let app = Router::new()
        .route("/wallet", get(wallet_handler))
        .route("/api/state", get(state_handler))
        .route("/api/send", post(send_handler))
        .with_state((state, mempool));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("🌍 Web Terminal running at: http://localhost:3000/wallet");
    axum::serve(listener, app).await.unwrap();
}

async fn wallet_handler() -> Html<&'static str> {
    Html(HTML_PAGE)
}

async fn state_handler(State((state, _)): State<(Arc<Mutex<AppState>>, Arc<Mutex<VecDeque<L2Transaction>>>)>) -> Json<AppState> {
    let s = state.lock().unwrap();
    Json(s.clone())
}

async fn send_handler(
    State((_, mempool)): State<(Arc<Mutex<AppState>>, Arc<Mutex<VecDeque<L2Transaction>>>)>,
    Json(payload): Json<TxRequest>,
) -> Json<String> {
    let mut mp = mempool.lock().unwrap();
    
    let sender = if payload.instruction.starts_with("Faucet") {
         let parts: Vec<&str> = payload.instruction.split_whitespace().collect();
         if parts.len() > 1 { parts[1].to_string() } else { payload.sender }
    } else {
        payload.sender
    };

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    mp.push_back(L2Transaction {
        sender: sender,
        instruction: payload.instruction,
        signature: payload.signature,
        pubkey: "web_terminal_dummy_key".to_string(),
        timestamp: timestamp,
    });
    Json("OK".to_string())
}