use clap::{Parser, Subcommand};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use std::fs;

#[derive(Parser)]
#[command(name = "lumen-cli")]
#[command(about = "CLI for Lumen L2 Financial Layer", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    CreateWallet,
    Mint {
        amount: u64,
        #[arg(short, long)]
        address: Option<String>,
    },
    Transfer {
        amount: u64,
        to: String,
    },
}

#[derive(Serialize, Deserialize, Debug)]
struct L2Transaction {
    sender: String,
    instruction: String,
    signature: String,
    timestamp: u64,
    pubkey: String,
}

#[derive(Serialize, Deserialize)]
struct Wallet {
    address: String,
    pubkey: String,
    privkey: String,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let client = Client::new();
    let rpc_url = "http://127.0.0.1:3000/submit";

    match cli.command {
        Commands::CreateWallet => {
            let wallet = Wallet {
                address: format!("0x{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..16].to_string()),
                pubkey: "mock_pubkey".to_string(),
                privkey: "mock_privkey".to_string(),
            };
            let json = serde_json::to_string_pretty(&wallet).unwrap();
            fs::write("keypair.json", json).expect("Unable to save wallet");
            println!("✅ Wallet created: {}", wallet.address);
        }
        Commands::Mint { amount, address } => {
            let wallet_data = fs::read_to_string("keypair.json").expect("Please run 'create-wallet' first");
            let wallet: Wallet = serde_json::from_str(&wallet_data).expect("Invalid wallet file");

            let target = address.unwrap_or(wallet.address.clone());
            let instruction = format!("Mint {} {}", amount, target);
            
            send_tx(&client, rpc_url, &wallet, instruction).await;
        }
        Commands::Transfer { amount, to } => {
            let wallet_data = fs::read_to_string("keypair.json").expect("Please run 'create-wallet' first");
            let wallet: Wallet = serde_json::from_str(&wallet_data).expect("Invalid wallet file");

            let instruction = format!("Transfer {} {}", amount, to);
            send_tx(&client, rpc_url, &wallet, instruction).await;
        }
    }
}

async fn send_tx(client: &Client, url: &str, wallet: &Wallet, instruction: String) {
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

    let tx = L2Transaction {
        sender: wallet.address.clone(),
        instruction: instruction.clone(),
        signature: "mock_sig_valid".to_string(),
        timestamp,
        pubkey: wallet.pubkey.clone(),
    };

    println!("📤 Sending: '{}'...", instruction);
    
    let res = client.post(url)
        .json(&tx)
        .send()
        .await;

    match res {
        Ok(response) => {
            if response.status().is_success() {
                println!("✅ Transaction sent successfully!");
            } else {
                println!("❌ Failed: {:?}", response.status());
            }
        },
        Err(e) => println!("❌ Connection error: {}", e),
    }
}