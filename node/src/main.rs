use std::sync::Arc;
mod da_adapter;
mod rpc;
mod mempool;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    
    println!("--------------------------------------------------");
    println!("🚀 Starting Lumen-btc-l2 (Phase 2.1 Sync)");
    println!("--------------------------------------------------");

    // 1. Initialize shared mempool
    let mempool = mempool::init_mempool();
    let da_layer = da_adapter::BitcoinDAAdapter::new("Bitcoin Signet");

    // 2. Spawn RPC server
    let rpc_mempool = Arc::clone(&mempool);
    tokio::spawn(async move {
        rpc::start_rpc_server(rpc_mempool).await;
    });

    println!("⚙️ Sequencer active. Watching mempool...");

    // 3. Sequencer Loop (runs every 10s)
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        
        let mut q = mempool.lock().unwrap();
        if !q.is_empty() {
            println!("📦 Sequencer: Batching {} transactions...", q.len());

            let batch_data = format!("{:?}", *q); 
            q.clear(); // Flush mempool

            // Record batch on Bitcoin
            let btc_txid = da_layer.submit_batch(batch_data.as_bytes()).await;
            println!("✅ Batch anchored! Bitcoin TxID: {}", btc_txid);
        }
    }
}