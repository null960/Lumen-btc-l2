use reqwest::Client;
use serde::Deserialize;
use std::error::Error;
use std::time::Duration;

const API_BASE: &str = "https://mempool.space/testnet/api";

#[derive(Clone)]
pub struct BtcApi {
    client: Client,
}

#[derive(Deserialize, Debug)]
struct AddressStats {
    chain_stats: Stats,
    mempool_stats: Stats,
}

#[derive(Deserialize, Debug)]
struct Stats {
    funded_txo_sum: u64,
    spent_txo_sum: u64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Utxo {
    pub txid: String,
    pub vout: u32,
    pub value: u64,
    pub status: UtxoStatus,
}

#[derive(Deserialize, Debug, Clone)]
pub struct UtxoStatus {
    pub confirmed: bool,
}

impl BtcApi {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    pub async fn get_utxos(&self, address: &str) -> Result<Vec<Utxo>, Box<dyn Error>> {
        let url = format!("{}/address/{}/utxo", API_BASE, address);
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(format!("API Error: {}", resp.status()).into());
        }
        let utxos: Vec<Utxo> = resp.json().await?;
        Ok(utxos)
    }

    pub async fn broadcast_tx(&self, tx_hex: String) -> Result<String, Box<dyn Error>> {
        let url = format!("{}/tx", API_BASE);
        let resp = self.client.post(&url).body(tx_hex).send().await?;
        
        if resp.status().is_success() {
            let txid = resp.text().await?;
            Ok(txid)
        } else {
            let err_text = resp.text().await?;
            Err(format!("Broadcast failed: {}", err_text).into())
        }
    }
}