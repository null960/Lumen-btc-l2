use std::sync::Arc;
use std::fs::OpenOptions;
use std::io::Write;

mod da_adapter;
mod rpc;
mod mempool;
mod state;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    
    println!("--------------------------------------------------");
    println!("🚀 Lumen-btc-l2: Phase 4 (State & Contracts)");
    println!("--------------------------------------------------");

    // 1. Initialize Memory (State), Mempool, and DA Connection
    let mempool = mempool::init_mempool();
    let app_state = state::init_state(); // <--- Initialize the State
    
    // Connect to Nubit (or mock)
    let da_layer = da_adapter::BitcoinDAAdapter::new("http://localhost:26659");

    let rpc_mempool = Arc::clone(&mempool);
    let rpc_state = Arc::clone(&app_state);
    tokio::spawn(async move {
        rpc::start_rpc_server(rpc_mempool, rpc_state).await;
    });

    println!("🧠 VM Initialized. Global Counter starts at 0.");

    // 2. Main Sequencer Loop
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        
        let mut q = mempool.lock().unwrap();
        if !q.is_empty() {
            println!("⚡ Processing block with {} transactions...", q.len());

            // --- EXECUTION LAYER (The Smart Contract Logic) ---
            {
                // Lock the state to update it safely
                let mut state = app_state.lock().unwrap();
                
                for tx in q.iter() {
                    state.total_transactions += 1;

                    // This is our first "Smart Contract" logic:
                    // If the instruction is "Increment", we update the state.
                    if tx.instruction == "Increment" {
                        state.smart_contract_counter += 1;
                        println!("🤖 CONTRACT: Counter INCREMENTED by {}. New Value: {}", 
                            &tx.sender[..8], state.smart_contract_counter);
                    }
                }
                println!("📊 STATE STATUS: Total Txs: {} | Counter: {}", 
                    state.total_transactions, state.smart_contract_counter);
            }
            // -----------------------------------------------------

            // 3. Data Availability (Send batch to Bitcoin/Nubit)
            let batch_json = serde_json::to_string(&*q).unwrap();
            let tx_count = q.len();
            q.clear(); 

            match da_layer.submit_batch(&batch_json).await {
                Ok(nubit_hash) => {
                    let log_entry = format!("Anchor | Txs: {} | DA: {}\n", tx_count, nubit_hash);
                    let mut file = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open("batches.log")
                        .unwrap();
                    file.write_all(log_entry.as_bytes()).unwrap();
                },
                Err(e) => println!("❌ DA Error: {}", e),
            }
        }
    }
}