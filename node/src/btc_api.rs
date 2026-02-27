use reqwest::Client;
use serde::Deserialize;
use std::error::Error;
use std::time::Duration;

const API_BASE: &str = "https://mempool.space/testnet/api";

#[derive(Clone)]
pub struct BtcApi {
    client: Client,
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

pub type ApiResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

impl BtcApi {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Fetches Unspent Transaction Outputs for an address
    pub async fn get_utxos(&self, address: &str) -> ApiResult<Vec<Utxo>> {
        let url = format!("{}/address/{}/utxo", API_BASE, address);
        
        let resp = self.client.get(&url)
            .send()
            .await
            .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        
        if !resp.status().is_success() {
            return Err(format!("API Error: {}", resp.status()).into());
        }
        
        let utxos: Vec<Utxo> = resp.json()
            .await
            .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
            
        Ok(utxos)
    }

    /// Broadcasts a raw transaction hex to the network
    pub async fn broadcast_tx(&self, tx_hex: String) -> ApiResult<String> {
        let url = format!("{}/tx", API_BASE);
        
        let resp = self.client.post(&url)
            .body(tx_hex)
            .send()
            .await
            .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        
        if resp.status().is_success() {
            let txid = resp.text()
                .await
                .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
            Ok(txid)
        } else {
            let err_text = resp.text()
                .await
                .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
            Err(format!("Broadcast failed: {}", err_text).into())
        }
    }
}