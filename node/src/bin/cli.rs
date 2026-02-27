use clap::{Parser, Subcommand};
use serde_json::json;
use reqwest::Client;

#[derive(Parser)]
#[command(name = "Lumen CLI", version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Info
    Info,
    /// Faucet
    Faucet { address: Option<String> },
    /// Balance
    Balance { address: String },
    /// Transfer
    Transfer { from: String, to: String, amount: u64 },
}

// Main
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let rpc_url = std::env::var("LUMEN_RPC").unwrap_or_else(|_| "http://194.15.112.56:3000".to_string());
    let client = Client::new();

    match &cli.command {
        Commands::Info => {
            println!("--------------------------------------------------");
            println!("🚀 Lumen L2 CLI Tool");
            println!("📡 Remote Node: {}", rpc_url);
            
            let res = client.get(format!("{}/status", rpc_url)).send().await;
            match res {
                Ok(_) => println!("🟢 Status: Connected"),
                Err(_) => println!("🔴 Status: Node Unreachable"),
            }
            println!("--------------------------------------------------");
        }
        Commands::Faucet { address } => {
            println!("⏳ Requesting funds from faucet...");
            let res = client.post(format!("{}/faucet", rpc_url))
                .json(&json!({ "address": address }))
                .send()
                .await?;
            
            if res.status().is_success() {
                let json: serde_json::Value = res.json().await?;
                println!("✅ Success!");
                println!("💳 Address: {}", json["address"].as_str().unwrap_or(""));
                println!("💰 Amount: {} LUM", json["amount"].as_u64().unwrap_or(0));
                println!("📝 Message: {}", json["msg"].as_str().unwrap_or(""));
            } else {
                println!("❌ Faucet request failed: {}", res.status());
            }
        }
        Commands::Balance { address } => {
            let res = client.get(format!("{}/balance/{}", rpc_url, address))
                .send()
                .await?;
            println!("🏦 Balance for {}: {}", address, res.text().await?);
        }
        Commands::Transfer { from, to, amount } => {
            println!("⏳ Initiating transfer...");
            let res = client.post(format!("{}/transfer", rpc_url))
                .json(&json!({ "from": from, "to": to, "amount": amount }))
                .send()
                .await?;
            
            if res.status().is_success() {
                let json: serde_json::Value = res.json().await?;
                println!("✅ Transfer Response: {}", json["msg"].as_str().unwrap_or(""));
            } else {
                println!("❌ Transfer failed: {}", res.status());
            }
        }
    }
    Ok(())
}