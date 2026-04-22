use std::error::Error;
use crate::state::AppState;
use redb::{Database, ReadableTable, TableDefinition};

// Table
const STATE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("state");

// DB
pub struct Storage {
    db: Database,
}

impl Storage {
    // Open
    pub fn new(path: &str) -> Self {
        let db = Database::create(path).expect("Failed");
        Self { db }
    }

    // Write
    pub fn save_state(&self, state: &AppState) -> Result<(), Box<dyn Error>> {
        let encoded = bincode::serialize(state)?;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(STATE_TABLE)?;
            table.insert("current", encoded.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    // Read
    pub fn load_state(&self) -> Option<AppState> {
        let read_txn = self.db.begin_read().ok()?;
        let table = read_txn.open_table(STATE_TABLE).ok()?;
        let data = table.get("current").ok()??;
        bincode::deserialize(data.value()).ok()
    }
}