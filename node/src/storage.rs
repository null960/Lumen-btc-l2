use std::error::Error;
use crate::state::AppState;
use redb::{Database, ReadableTable, TableDefinition}; 

const STATE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("state");

pub struct Storage {
    db: Database,
}

impl Storage {
    pub fn new(path: &str) -> Self {
        let db = Database::builder()
            .create(path)
            .expect("Failed to open database");
        Self { db }
    }

    pub fn save_state(&self, state: &AppState) -> Result<(), Box<dyn Error>> {
        let encoded = bincode::serialize(state)?;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(STATE_TABLE)?;
            table.insert("current_state", encoded.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn load_state(&self) -> Option<AppState> {
        let read_txn = self.db.begin_read().ok()?;
        let table = read_txn.open_table(STATE_TABLE).ok()?;
        let data = table.get("current_state").ok()??;
        bincode::deserialize(data.value()).ok()
    }
}