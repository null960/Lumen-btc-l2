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
    CreateWallet,
    Transfer {
        #[arg(short, long)]
        to: String,
        #[arg(short, long)]
        amount: u64,
    },
    Increment,
}

#[derive(Serialize)]
struct L2Transaction {
    sender: String,
    instruction: String,
    amount: u64,
    signature: String,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::CreateWallet => {
            let keypair = Keypair::new();
            keypair.write_to_file("keypair.json").expect("Failed to write keypair");
            println!("✨ Wallet created! PubKey: {}", keypair.pubkey());
        }
        
        Commands::Transfer { to: _, amount } => {
            process_tx("Transfer", *amount).await;
        }

        Commands::Increment => {
            process_tx("Increment", 0).await;
        }
    }
}

// Helper function to avoid code duplication
async fn process_tx(instruction_type: &str, amount: u64) {
    let keypair = Keypair::read_from_file("keypair.json").expect("No keypair.json found!");
    
    // Usually, instruction data is complex, but here we use a string for MVP
    let instruction = if instruction_type == "Increment" {
        "Increment".to_string()
    } else {
        format!("Transfer (amount: {})", amount)
    };
    
    // Sign the instruction
    let signature = keypair.sign_message(instruction.as_bytes()).to_string();

    let tx = L2Transaction {
        sender: keypair.pubkey().to_string(),
        instruction: instruction.clone(),
        amount,
        signature,
    };

    let client = Client::new();
    println!("🚀 Sending '{}' to Node...", instruction_type);
    
    let res = client.post("http://127.0.0.1:3000/submit")
        .json(&tx)
        .send()
        .await;

    match res {
        Ok(response) => {
            println!("📡 Node Response: {}", response.status());
        }
        Err(e) => println!("❌ Connection Error: {}", e),
    }
}