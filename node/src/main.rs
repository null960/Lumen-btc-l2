use std::sync::Arc;
use std::fs::OpenOptions;
use std::io::Write;

mod da_adapter;
mod rpc;
mod mempool;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    
    println!("--------------------------------------------------");
    println!("🚀 Lumen-btc-l2: Phase 3.1 (Live DA Integration)");
    println!("--------------------------------------------------");

    let mempool = mempool::init_mempool();
    // Initialize adapter with Nubit RPC endpoint
    let da_layer = da_adapter::BitcoinDAAdapter::new("http://localhost:26659");

    let rpc_mempool = Arc::clone(&mempool);
    tokio::spawn(async move {
        rpc::start_rpc_server(rpc_mempool).await;
    });

    loop {
        // Check mempool every 10 seconds
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        
        let mut q = mempool.lock().unwrap();
        if !q.is_empty() {
            println!("📦 Sequencer: Batching {} transactions...", q.len());

            // Serialize transactions to JSON string for DA storage
            let batch_json = serde_json::to_string(&*q).unwrap();
            let tx_count = q.len();
            q.clear(); // Flush mempool after batching

            // Submit batch to Nubit DA layer
            match da_layer.submit_batch(&batch_json).await {
                Ok(nubit_hash) => {
                    // Record proof of data availability in local log
                    let log_entry = format!(
                        "[{}] Block #{} Anchored | Transactions: {} | DA_Hash: {} | Status: Finalized\n",
                        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                        tx_count + 100, // Just a mock block number
                        tx_count,
                        nubit_hash
                    );
                    let mut file = OpenOptions::new().create(true).append(true).open("batches.log").unwrap();
                    file.write_all(log_entry.as_bytes()).unwrap();
                    
                    println!("✅ Success: Batch anchored! Hash: {}", nubit_hash);
                },
                Err(e) => println!("❌ DA Error: Failed to anchor batch: {}", e),
            }
        }
    }
}