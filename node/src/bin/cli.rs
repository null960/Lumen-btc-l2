//! Lumen CLI — command line interface for Lumen Network
//! 
//! Usage:
//!   LUMEN_RPC=http://... cargo run --bin cli -- <command>

use clap::{Parser, Subcommand};
use serde_json::json;
use reqwest::Client;

#[derive(Parser)]
#[command(name = "lumen-cli", version = "2.0.0", about = "Lumen Network CLI — Bitcoin L2 with LSAT")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show node info and network stats
    Info,

    /// Get LSAT balance for an address
    Balance { address: String },

    /// Request free testnet LSAT from faucet
    Faucet { address: String },

    /// Transfer LSAT to another address (free, instant)
    Transfer {
        from: String,
        to: String,
        amount: u64,
        #[arg(long)] memo: Option<String>,
    },

    /// Withdraw LSAT to Bitcoin L1 (24h challenge window)
    Withdraw {
        from: String,
        #[arg(long)] btc_address: String,
        amount: u64,
    },

    /// Register an app (game, shop) on Lumen Network
    RegisterApp {
        owner: String,
        #[arg(long)] app_id: String,
        #[arg(long)] app_name: String,
        #[arg(long)] token: String,
        #[arg(long)] rate: u64,
    },

    /// Buy app tokens with LSAT
    BuyToken {
        buyer: String,
        #[arg(long)] app_id: String,
        #[arg(long)] amount: u64,
    },

    /// List all registered apps
    Apps,

    /// Show pending withdrawals (PegOuts in challenge window)
    Withdrawals,

    /// Get Merkle proof of LSAT balance
    Proof { address: String },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let rpc = std::env::var("LUMEN_RPC").unwrap_or_else(|_| "http://localhost:3000".to_string());
    let client = Client::new();

    match &cli.command {
        Commands::Info => {
            println!("┌─────────────────────────────────────────┐");
            println!("│  🟠 Lumen Network CLI v2.0               │");
            println!("│  1 LSAT = 1 Bitcoin Satoshi              │");
            println!("└─────────────────────────────────────────┘");
            println!("📡 Node: {}", rpc);

            let res = client.get(format!("{}/api/state", rpc)).send().await;
            match res {
                Ok(r) if r.status().is_success() => {
                    let d: serde_json::Value = r.json().await?;
                    println!("🟢 Status: ONLINE");
                    println!("📦 Block Height: #{}", d["block_height"].as_u64().unwrap_or(0));
                    println!("📊 Transactions: {}", d["total_transactions"].as_u64().unwrap_or(0));
                    println!("🔗 State Root: {}", d["latest_state_root"].as_str().unwrap_or("—"));
                    println!("👥 Accounts: {}", d["stats"]["total_accounts"].as_u64().unwrap_or(0));
                    println!("🎮 Apps: {}", d["apps"].as_array().map(|a| a.len()).unwrap_or(0));
                },
                _ => println!("🔴 Status: UNREACHABLE"),
            }
        }

        Commands::Balance { address } => {
            // Try new JSON endpoint first
            let res = client.get(format!("{}/api/balance/{}", rpc, address)).send().await?;
            let text = res.text().await?;
            // Handle both JSON (new node) and plain text (old node)
            if let Ok(d) = serde_json::from_str::<serde_json::Value>(&text) {
                println!("Address : {}", address);
                println!("LSAT    : {}", d["lsat"].as_u64().unwrap_or(0));
                println!("BTC     : {}", d["btc_equivalent"].as_str().unwrap_or("—"));
                if let Some(tokens) = d["app_tokens"].as_object() {
                    if !tokens.is_empty() {
                        println!("Tokens  :");
                        for (k, v) in tokens { println!("  {} : {}", k, v); }
                    }
                }
            } else {
                // Old node returns plain text like "50000 LSAT"
                println!("Address : {}", address);
                println!("Balance : {}", text.trim());
            }
        }

        Commands::Faucet { address } => {
            println!("⏳ Requesting faucet...");
            let res = client.post(format!("{}/faucet", rpc))
                .json(&json!({ "address": address }))
                .send().await?;
            let d: serde_json::Value = res.json().await?;
            if d["status"] == "success" {
                println!("✅ {} LSAT sent to {}", d["amount"].as_u64().unwrap_or(0), address);
            } else {
                println!("❌ {}", d["msg"].as_str().unwrap_or("Error"));
            }
        }

        Commands::Transfer { from, to, amount, memo } => {
            println!("⏳ Submitting transfer...");
            let res = client.post(format!("{}/transfer", rpc))
                .json(&json!({ "from": from, "to": to, "amount": amount, "memo": memo }))
                .send().await?;
            let d: serde_json::Value = res.json().await?;
            if d["status"] == "ok" || d["status"] == "success" {
                println!("✅ Transferred {} LSAT from {} to {}", amount, &from[..10], &to[..10]);
                if let Some(m) = memo { println!("   Memo: {}", m); }
            } else {
                println!("❌ {}", d["msg"].as_str().unwrap_or("Error"));
            }
        }

        Commands::Apps => {
            let res = client.get(format!("{}/api/apps", rpc)).send().await?;
            let d: serde_json::Value = res.json().await?;
            let apps = d["apps"].as_array().cloned().unwrap_or_default();
            if apps.is_empty() {
                println!("No apps registered yet.");
            } else {
                println!("┌─── Registered Apps ──────────────────────────┐");
                for app in &apps {
                    println!("  {} — {} Token", app["app_id"].as_str().unwrap_or("?"), app["token_name"].as_str().unwrap_or("?"));
                    println!("    Rate: 1 LSAT = {} {}", app["rate_per_lsat"].as_u64().unwrap_or(0), app["token_name"].as_str().unwrap_or("?"));
                    println!("    Collected: {} LSAT", app["lsat_collected"].as_u64().unwrap_or(0));
                    println!();
                }
            }
        }

        Commands::Withdrawals => {
            let res = client.get(format!("{}/api/withdrawals", rpc)).send().await?;
            let d: serde_json::Value = res.json().await?;
            let wds = d["withdrawals"].as_array().cloned().unwrap_or_default();
            if wds.is_empty() {
                println!("No pending withdrawals.");
            } else {
                for wd in &wds {
                    println!("ID: {} | {} LSAT → {} | Status: {}",
                        wd["id"].as_str().unwrap_or("?"),
                        wd["amount_lsat"].as_u64().unwrap_or(0),
                        wd["btc_address"].as_str().unwrap_or("?"),
                        wd["status"].as_str().unwrap_or("?"));
                }
            }
        }

        Commands::Proof { address } => {
            let res = client.get(format!("{}/api/proof/{}", rpc, address)).send().await?;
            let d: serde_json::Value = res.json().await?;
            if d["status"] == "ok" {
                println!("✅ Merkle Proof for {}", address);
                println!("   Balance: {} LSAT", d["balance_lsat"].as_u64().unwrap_or(0));
                println!("   State Root: {}", d["state_root"].as_str().unwrap_or("—"));
            } else {
                println!("❌ {}", d["msg"].as_str().unwrap_or("Address not found"));
            }
        }

        _ => println!("Command submitted. Use the web dashboard at {}", rpc),
    }

    Ok(())
}