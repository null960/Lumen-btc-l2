use redb::{Database, ReadableTable, TableDefinition};
use std::error::Error;

use lumen_common::models::Merchant;

const MERCHANTS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("merchants");
const INVOICES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("invoices");
const PAYMENTS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("payments");

pub struct Storage {
    db: Database,
}

impl Storage {
    pub fn new(path: &str) -> Result<Self, Box<dyn Error>> {
        let db = Database::create(path)?;
        
        let write_txn = db.begin_write()?;
        {
            write_txn.open_table(MERCHANTS_TABLE)?;
            write_txn.open_table(INVOICES_TABLE)?;
            write_txn.open_table(PAYMENTS_TABLE)?;
        }
        write_txn.commit()?;

        Ok(Self { db })
    }

    pub fn save_merchant(&self, merchant: &Merchant) -> Result<(), Box<dyn Error>> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(MERCHANTS_TABLE)?;
            let encoded = bincode::serialize(merchant)?;
            table.insert(merchant.id.as_str(), encoded.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn get_merchant(&self, id: &str) -> Result<Option<Merchant>, Box<dyn Error>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(MERCHANTS_TABLE)?;
        let result = if let Some(data) = table.get(id)? {
            let merchant: Merchant = bincode::deserialize(data.value())?;
            Some(merchant)
        } else {
            None
        };
        
        Ok(result)
    }

}