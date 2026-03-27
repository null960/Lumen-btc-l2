use std::fs;
use std::path::Path;
use std::error::Error;
use bitcoin::hashes::{sha256, Hash};
use crate::state::{TxRecord, AppState};
use chrono::Utc;

// ── Batch format ──────────────────────────────────────────────────────────────

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct DaBatch {
    pub id:           u64,
    pub timestamp:    i64,
    pub hash:         String,
    pub state_root:   String,
    pub transactions: Vec<TxRecord>,
}

type DaResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

// ── BitcoinDAAdapter ──────────────────────────────────────────────────────────

pub struct BitcoinDAAdapter {
    pub storage_path: String,
}

impl BitcoinDAAdapter {
    pub fn new(storage_path: &str) -> Self {
        if !Path::new(storage_path).exists() {
            fs::create_dir_all(storage_path).unwrap();
        }
        Self { storage_path: storage_path.to_string() }
    }

    // ── Write operations ──────────────────────────────────────────────────────

    /// Commit a full batch of 5+ transactions to permanent storage
    pub fn submit_batch_with_root(
        &self, batch_id: u64, txs: &[TxRecord], state_root: &str,
    ) -> DaResult<String> {
        let batch = DaBatch {
            id:           batch_id,
            timestamp:    Utc::now().timestamp(),
            hash:         String::new(),
            state_root:   state_root.to_string(),
            transactions: txs.to_vec(),
        };

        let json = serde_json::to_string(&batch)?;
        let hash = sha256::Hash::hash(json.as_bytes()).to_string();

        let batch_final = DaBatch { hash: hash.clone(), ..batch };
        let json_final  = serde_json::to_string(&batch_final)?;

        let path = format!(
            "{}/batch_{}_{}.json",
            self.storage_path, batch_id,
            &hash[..hash.len().min(12)]
        );
        fs::write(&path, json_final)?;

        println!("📦 Batch #{} committed | {} txs | {}...",
            batch_id, txs.len(), &hash[..hash.len().min(12)]);
        Ok(hash)
    }

    /// Write pending batch after every tx — ensures no tx is lost on crash
    pub fn save_pending_batch(
        &self, batch_id: u64, txs: &[TxRecord], state_root: &str,
    ) -> DaResult<()> {
        let batch = DaBatch {
            id:           batch_id,
            timestamp:    Utc::now().timestamp(),
            hash:         String::new(),
            state_root:   state_root.to_string(),
            transactions: txs.to_vec(),
        };
        let path = format!("{}/pending_{}.json", self.storage_path, batch_id);
        fs::write(path, serde_json::to_string(&batch)?)?;
        Ok(())
    }

    /// Remove pending file once full batch is committed
    pub fn clear_pending_batch(&self, batch_id: u64) -> DaResult<()> {
        let path = format!("{}/pending_{}.json", self.storage_path, batch_id);
        if Path::new(&path).exists() { fs::remove_file(path)?; }
        Ok(())
    }

    /// Save full state snapshot for fast recovery
    pub fn save_snapshot(&self, state: &AppState) -> DaResult<String> {
        let json = serde_json::to_string_pretty(state)?;
        let hash = sha256::Hash::hash(json.as_bytes()).to_string();
        let ts   = Utc::now().timestamp();
        let path = format!(
            "{}/snapshot_{}_{}.json",
            self.storage_path, ts,
            &hash[..hash.len().min(12)]
        );
        fs::write(&path, &json)?;
        println!("💾 Snapshot saved: {}", path);
        Ok(hash)
    }

    // ── Read operations ───────────────────────────────────────────────────────

    /// Load all committed batches sorted by id (oldest first)
    pub fn load_all_batches(&self) -> Vec<DaBatch> {
        let mut batches = Vec::new();

        let Ok(entries) = fs::read_dir(&self.storage_path) else {
            return batches;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None    => continue,
            };
            if !name.ends_with(".json")     { continue; }
            if name.starts_with("snapshot_"){ continue; }
            if name.starts_with("pending_") { continue; }

            let Ok(content) = fs::read_to_string(&path) else { continue };

            if let Ok(batch) = serde_json::from_str::<DaBatch>(&content) {
                batches.push(batch);
            } else if let Ok(txs) = serde_json::from_str::<Vec<TxRecord>>(&content) {
                // Backward compat with old format
                let id = name.split('_').nth(1)
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0);
                batches.push(DaBatch {
                    id, timestamp: 0, hash: String::new(),
                    state_root: String::new(), transactions: txs,
                });
            }
        }

        batches.sort_by_key(|b| b.id);
        batches
    }

    /// Load most recent snapshot
    pub fn load_latest_snapshot(&self) -> Option<AppState> {
        let mut snapshots: Vec<(i64, std::path::PathBuf)> =
            fs::read_dir(&self.storage_path).ok()?
                .flatten()
                .filter_map(|e| {
                    let path = e.path();
                    let name = path.file_name()?.to_str()?.to_string();
                    if !name.starts_with("snapshot_") { return None; }
                    let ts = name.split('_').nth(1)?.parse::<i64>().ok()?;
                    Some((ts, path))
                })
                .collect();

        snapshots.sort_by(|a, b| b.0.cmp(&a.0)); // newest first
        let (ts, path) = snapshots.first()?;
        let content = fs::read_to_string(path).ok()?;
        let state: AppState = serde_json::from_str(&content).ok()?;
        println!("📸 Snapshot loaded (ts: {})", ts);
        Some(state)
    }

    /// Load pending (incomplete) batch — highest id wins
    pub fn load_pending_batch(&self) -> Option<DaBatch> {
        let mut pending: Vec<(u64, std::path::PathBuf)> =
            fs::read_dir(&self.storage_path).ok()?
                .flatten()
                .filter_map(|e| {
                    let path = e.path();
                    let name = path.file_name()?.to_str()?.to_string();
                    if !name.starts_with("pending_") { return None; }
                    let id = name.strip_prefix("pending_")?
                        .strip_suffix(".json")?
                        .parse::<u64>().ok()?;
                    Some((id, path))
                })
                .collect();

        pending.sort_by_key(|(id, _)| *id);
        let (_, path) = pending.last()?;
        serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
    }

    /// Count committed batches
    pub fn batch_count(&self) -> usize {
        fs::read_dir(&self.storage_path)
            .map(|e| e.flatten().filter(|f| {
                let n = f.file_name();
                let n = n.to_str().unwrap_or("");
                n.ends_with(".json")
                    && !n.starts_with("snapshot_")
                    && !n.starts_with("pending_")
            }).count())
            .unwrap_or(0)
    }
}