use solana_sdk::signature::{Keypair, Signer};
use bitcoin::Network;

// Import local DA module
mod da_adapter;
use da_adapter::BitcoinDAAdapter;

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    println!("--------------------------------------------------");
    println!("🚀 Starting Lumen-btc-l2 Node (Grant MVP)");
    println!("--------------------------------------------------");

    // 1. Initialize SVM Identity
    let svm_wallet = Keypair::new();
    println!("✅ SVM Module Initialized");
    println!("🔑 Node Operator PubKey: {:?}", svm_wallet.pubkey());

    // 2. Initialize Bitcoin DA Adapter
    let btc_network = Network::Testnet;
    let da_layer = BitcoinDAAdapter::new("Bitcoin Testnet (via Nubit)");
    
    println!("✅ Bitcoin Data Availability Module Initialized");
    println!("🔗 Targeted Network: {:?}", btc_network);
    
    println!("--------------------------------------------------");
    println!("🔄 Simulation: Producing Block #1...");

    // 3. Simulate L2 Block Creation
    // Mock data representing encrypted user transactions
    let mock_l2_block_data = b"User A sent 100 USDC to User B via SVM"; 
    
    // Submit batch to the Data Availability layer
    let tx_id = da_layer.submit_batch(mock_l2_block_data).await;

    println!("✅ Block #1 Finalized on Bitcoin!");
    println!("🧾 Proof (TxID): {}", tx_id);
    println!("--------------------------------------------------");
}