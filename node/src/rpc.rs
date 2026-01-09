use axum::{
    routing::{get, post},
    Router, 
    Json, 
    response::{Html, IntoResponse}, 
    extract::State,
};
use std::sync::{Arc, Mutex};
use crate::state::AppState;
use crate::mempool::Mempool;
use serde::Deserialize;
use serde_json::json;

#[derive(Clone)]
pub struct ServerState {
    pub app_state: Arc<Mutex<AppState>>,
    pub mempool: Arc<Mutex<Mempool>>,
}

#[derive(Deserialize)]
pub struct UserCommand {
    pub cmd: String,
}

pub async fn run_server(state: Arc<Mutex<AppState>>, mempool: Arc<Mutex<Mempool>>) {
    let shared_state = ServerState {
        app_state: state,
        mempool,
    };

    let app = Router::new()
        .route("/wallet", get(wallet_ui))
        .route("/api/state", get(get_state))
        .route("/api/cmd", post(submit_command))
        .with_state(shared_state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}


async fn submit_command(
    State(state): State<ServerState>,
    Json(payload): Json<UserCommand>,
) -> Json<serde_json::Value> {
    let mut mempool = state.mempool.lock().unwrap();
    println!("📩 RPC Received: {}", payload.cmd);
    mempool.queue.push_back(payload.cmd);
    Json(json!({ "status": "ok" }))
}

async fn get_state(State(state): State<ServerState>) -> Json<serde_json::Value> {
    let app_state = state.app_state.lock().unwrap();
    Json(json!({
        "total_transactions": app_state.total_transactions,
        "balances": app_state.balances,
        "network": "Testnet",
        "status": "Online" 
    }))
}

async fn wallet_ui() -> impl IntoResponse {
    let html = r#"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Lumen L2 Command Center</title>
    <style>
        :root {
            --bg-color: #050505;
            --panel-bg: #111;
            --border: #333;
            --accent: #00ff41; /* Hacker Green */
            --text: #e0e0e0;
            --danger: #ff3333;
        }
        body { margin: 0; background-color: var(--bg-color); color: var(--text); font-family: 'Courier New', monospace; overflow: hidden; height: 100vh; display: grid; grid-template-columns: 300px 1fr; }
        
        /* SIDEBAR */
        .sidebar { background: var(--panel-bg); border-right: 1px solid var(--border); padding: 20px; display: flex; flex-direction: column; gap: 20px; }
        .logo { font-size: 24px; font-weight: bold; color: var(--text); border-bottom: 1px solid var(--border); padding-bottom: 15px; }
        .logo span { color: var(--accent); }
        
        .stat-box { background: #000; border: 1px solid var(--border); padding: 15px; border-radius: 4px; }
        .stat-label { font-size: 12px; color: #888; text-transform: uppercase; margin-bottom: 5px; }
        .stat-value { font-size: 20px; font-weight: bold; color: var(--accent); }
        .stat-value.btc { color: #f2a900; }

        .status-dot { height: 10px; width: 10px; background-color: var(--accent); border-radius: 50%; display: inline-block; margin-right: 8px; box-shadow: 0 0 5px var(--accent); }

        /* MAIN TERMINAL */
        .main { display: flex; flex-direction: column; padding: 20px; gap: 10px; height: 100vh; box-sizing: border-box; }
        
        .terminal-window { 
            flex-grow: 1; 
            background: #000; 
            border: 1px solid var(--border); 
            border-radius: 4px; 
            padding: 15px; 
            overflow-y: auto; 
            font-size: 14px;
            line-height: 1.5;
            box-shadow: inset 0 0 20px rgba(0,0,0,0.8);
        }
        
        .log-entry { margin-bottom: 5px; opacity: 0; animation: fadeIn 0.2s forwards; }
        .log-time { color: #555; margin-right: 10px; }
        .cmd-input-container { display: flex; gap: 10px; background: var(--panel-bg); padding: 10px; border: 1px solid var(--border); border-radius: 4px; align-items: center; }
        .prompt { color: var(--accent); font-weight: bold; }
        input { background: transparent; border: none; color: white; width: 100%; font-family: inherit; font-size: 16px; outline: none; }
        
        @keyframes fadeIn { from { opacity: 0; transform: translateY(5px); } to { opacity: 1; transform: translateY(0); } }

        /* SCROLLBAR */
        ::-webkit-scrollbar { width: 8px; }
        ::-webkit-scrollbar-track { background: #000; }
        ::-webkit-scrollbar-thumb { background: #333; border-radius: 4px; }
    </style>
</head>
<body>
    <div class="sidebar">
        <div class="logo">🟠 Lumen <span>L2</span></div>
        
        <div class="stat-box">
            <div class="stat-label">Network Status</div>
            <div style="display:flex; align-items:center;">
                <div class="status-dot"></div> Online
            </div>
            <div style="font-size: 11px; color: #555; margin-top: 5px;">Phase 3: Settlement Active</div>
        </div>

        <div class="stat-box">
            <div class="stat-label">My Balance (L2)</div>
            <div class="stat-value btc" id="balance">--- sats</div>
        </div>

        <div class="stat-box">
            <div class="stat-label">Total Transactions</div>
            <div class="stat-value" id="tx-count">0</div>
        </div>
        
        <div style="font-size: 12px; color: #444; margin-top: auto;">
            Commands:<br>
            - <span style="color:#888">Me</span><br>
            - <span style="color:#888">Transfer &lt;amt&gt; &lt;to&gt;</span><br>
            - <span style="color:#888">Withdraw &lt;amt&gt; &lt;addr&gt;</span>
        </div>
    </div>

    <div class="main">
        <div class="terminal-window" id="terminal">
            <div class="log-entry" style="color: var(--accent)">
                Initializing Lumen L2 Node Interface...
            </div>
            <div class="log-entry">Connected to Testnet via Mempool.space API.</div>
            <div class="log-entry">Waiting for input...</div>
            <br>
        </div>
        
        <div class="cmd-input-container">
            <div class="prompt">user@l2:~$</div>
            <input type="text" id="cmd" autofocus placeholder="Enter command here...">
        </div>
    </div>

    <script>
        const term = document.getElementById('terminal');
        const input = document.getElementById('cmd');
        const balanceEl = document.getElementById('balance');
        const txCountEl = document.getElementById('tx-count');

        function getTime() {
            const now = new Date();
            return now.toLocaleTimeString('en-US', { hour12: false });
        }

        function log(msg, color='#e0e0e0') {
            const div = document.createElement('div');
            div.className = 'log-entry';
            div.innerHTML = `<span class="log-time">[${getTime()}]</span> <span style="color:${color}">${msg}</span>`;
            term.appendChild(div);
            term.scrollTop = term.scrollHeight;
        }

        async function postCmd(cmd) {
            try {
                await fetch('/api/cmd', {
                    method: 'POST',
                    headers: {'Content-Type': 'application/json'},
                    body: JSON.stringify({ cmd: cmd })
                });
            } catch(e) { log('Error connecting to node', 'red'); }
        }

        async function updateState() {
            try {
                let res = await fetch('/api/state');
                let state = await res.json();
                
                // Анимация чисел
                txCountEl.innerText = state.total_transactions;
                
                let myBal = state.balances['0xUser'] || 0;
                balanceEl.innerText = myBal.toLocaleString() + ' sats';

            } catch(e) { 
                // Ignored
            }
        }

        // Poll state every 2 seconds
        setInterval(updateState, 2000);
        updateState();

        input.addEventListener('keypress', async (e) => {
            if (e.key === 'Enter') {
                let val = input.value.trim();
                if (!val) return;
                
                input.value = '';
                log(`> ${val}`, '#888');

                if (val.toLowerCase() === 'help') {
                    log('Available Commands:', '#f2a900');
                    log('  Me - Refresh balance');
                    log('  Transfer 100 0xBob - Send L2 funds');
                    log('  Withdraw 1000 mk... - Peg-out to Bitcoin L1');
                } 
                else if (val.toLowerCase() === 'me') {
                    updateState();
                    log('Balance updated.', 'var(--accent)');
                }
                else if (val.toLowerCase().startsWith('transfer')) {
                    await postCmd(val);
                    log('Transfer submitted to Mempool.', '#58a6ff');
                }
                else if (val.toLowerCase().startsWith('withdraw')) {
                    await postCmd(val);
                    log('Withdrawal request broadcasting...', '#ff3333');
                }
                else {
                    log('Unknown command. Type "Help".', 'red');
                }
            }
        });
    </script>
</body>
</html>
    "#;
    Html(html)
}