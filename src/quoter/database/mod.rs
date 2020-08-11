use super::{BlockProcessor, StateProvider};
use crate::side_chain::SideChainBlock;

use rusqlite;
use rusqlite::params;

use rusqlite::Connection;
use std::sync::Mutex;

mod migration;

/// A database for storing and accessing local state
#[derive(Debug)]
pub struct Database {
    db: Mutex<Connection>,
}

impl Database {
    /// Returns a database instance from the given path.
    pub fn open(file: &str) -> Self {
        let connection = Connection::open(file).expect("Could not open the database");
        return Database::new(connection);
    }

    /// Returns a database instance with the given connection.
    pub fn new(mut connection: Connection) -> Self {
        migration::migrate_database(&mut connection);

        Database {
            db: Mutex::new(connection),
        }
    }

    /// Get key value data
    fn get_data<T>(&self, key: &str) -> Option<T>
    where
        T: rusqlite::types::FromSql,
    {
        let db = self.db.lock().unwrap();
        let mut statement = match db.prepare("SELECT value from data WHERE key = ?1;") {
            Ok(statement) => statement,
            Err(_) => return None,
        };
        let val: Result<T, _> = statement.query_row(params![key], |row| row.get(0));
        match val {
            Ok(result) => Some(result),
            Err(_) => None,
        }
    }

    /// Set key value data
    fn set_data<T>(&self, key: &str, value: Option<T>) -> Result<(), String>
    where
        T: rusqlite::types::ToSql,
    {
        let db = self.db.lock().unwrap();
        match db.execute(
            "INSERT OR REPLACE INTO data (key, value) VALUES (?1, ?2)",
            params![key, value],
        ) {
            Ok(_) => Ok(()),
            Err(err) => {
                error!("Error inserting key values into data table: {}", err);
                return Err("Failed to set data.".to_owned());
            }
        }
    }

    fn set_last_processed_block_number(&self, block_number: u32) -> Result<(), String> {
        return self.set_data("last_processed_block_number", Some(block_number));
    }
}

impl BlockProcessor for Database {
    fn get_last_processed_block_number(&self) -> Option<u32> {
        self.get_data("last_processed_block_number")
    }

    fn process_blocks(&self, blocks: Vec<SideChainBlock>) -> Result<(), String> {
        let mut db = self.db.lock().unwrap();
        let tx = match db.transaction() {
            Ok(transaction) => transaction,
            Err(err) => {
                error!("Failed to open database transaction: {}", err);
                return Err("Failed to process block".to_owned());
            }
        };

        for block in blocks.iter() {
            // TODO: Do stuff here
        }

        tx.commit().or_else(|err| {
            error!("Failed to commit process block changes: {}", err);
            return Err("Failed to commit process block changes.");
        })?;

        let last_block_number = blocks.iter().map(|b| b.number).max();
        if let Some(last_block_number) = last_block_number {
            self.set_last_processed_block_number(last_block_number)?;
        }

        return Ok(());
    }
}

impl StateProvider for Database {}
