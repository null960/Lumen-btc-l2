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
    println!("🚀 Starting Lumen-btc-l2 (Phase 2.2: Secured)");
    println!("--------------------------------------------------");

    let mempool = mempool::init_mempool();
    let da_layer = da_adapter::BitcoinDAAdapter::new("Bitcoin Signet");

    let rpc_mempool = Arc::clone(&mempool);
    tokio::spawn(async move {
        rpc::start_rpc_server(rpc_mempool).await;
    });

    println!("⚙️ Sequencer active. Watching mempool...");

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        
        let mut q = mempool.lock().unwrap();
        if !q.is_empty() {
            println!("📦 Sequencer: Batching {} verified transactions...", q.len());

            let batch_data = format!("{:?}", *q); 
            q.clear(); 

            let btc_txid = da_layer.submit_batch(batch_data.as_bytes()).await;
            
            // Log for future Block Explorer
            let log_entry = format!("Block Anchored | BtcTxID: {}\n", btc_txid);
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open("batches.log")
                .expect("Failed to open log file");
            
            file.write_all(log_entry.as_bytes()).ok();

            println!("✅ Batch anchored to Bitcoin! TxID: {}", btc_txid);
        }
    }
}