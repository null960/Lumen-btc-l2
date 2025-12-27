use clap::{Parser, Subcommand};
use solana_sdk::signature::{Keypair, EncodableKey, Signer};
use serde::Serialize;
use reqwest::Client;

#[derive(Parser)]
#[command(name = "lumen-cli")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new wallet
    CreateWallet,
    /// Send a transaction to the L2 Node
    Transfer {
        #[arg(short, long)]
        to: String,
        #[arg(short, long)]
        amount: u64,
    },
}

#[derive(Serialize)]
struct L2Transaction {
    sender: String,
    instruction: String,
    amount: u64,
    signature: String,
}

#[tokio::main] // Added for async HTTP requests
async fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::CreateWallet => {
            let keypair = Keypair::new();
            keypair.write_to_file("keypair.json").expect("Failed to write keypair");
            println!("✨ Wallet created! PubKey: {}", keypair.pubkey());
        }
        Commands::Transfer { to, amount } => {
            // 1. Load your keypair
            let keypair = Keypair::read_from_file("keypair.json").expect("No keypair.json found!");
            
            // 2. Prepare the instruction (message to sign)
            let instruction = format!("Transfer to {}", to);
            
            // 3. Create cryptographic signature
            let signature = keypair.sign_message(instruction.as_bytes()).to_string();

            // 4. Build the JSON payload
            let tx = L2Transaction {
                sender: keypair.pubkey().to_string(),
                instruction,
                amount: *amount,
                signature,
            };

            // 5. Send to the Node RPC
            let client = Client::new();
            println!("🚀 Sending tx to http://127.0.0.1:3000/submit...");
            
            let res = client.post("http://127.0.0.1:3000/submit")
                .json(&tx)
                .send()
                .await;

            match res {
                Ok(response) => {
                    let status = response.status();
                    let body = response.text().await.unwrap();
                    println!("📡 Node Response [{}]: {}", status, body);
                }
                Err(e) => println!("❌ Failed to connect to node: {}", e),
            }
        }
    }
}