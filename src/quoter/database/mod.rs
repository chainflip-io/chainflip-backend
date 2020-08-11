use super::{BlockProcessor, StateProvider};
use crate::side_chain::SideChainBlock;

use rusqlite;
use rusqlite::params;

use rusqlite::Connection;

mod migration;

/// A database for storing and accessing local state
#[derive(Debug)]
pub struct Database {
    db: Connection,
}

impl Database {
    /// Returns a database instance from the given path.
    pub fn open(file: &str) -> Self {
        let connection = Connection::open(file).expect("Could not open the database");
        Database::new(connection)
    }

    /// Returns a database instance with the given connection.
    pub fn new(mut connection: Connection) -> Self {
        migration::migrate_database(&mut connection);
        Database { db: connection }
    }

    /// Get key value data
    fn get_data<T>(&self, key: &str) -> Option<T>
    where
        T: rusqlite::types::FromSql,
    {
        let mut statement = match self.db.prepare("SELECT value from data WHERE key = ?1;") {
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
        match self.db.execute(
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
        self.set_data("last_processed_block_number", Some(block_number))
    }
}

impl BlockProcessor for Database {
    fn get_last_processed_block_number(&self) -> Option<u32> {
        self.get_data("last_processed_block_number")
    }

    fn process_blocks(&mut self, blocks: Vec<SideChainBlock>) -> Result<(), String> {
        let tx = match self.db.transaction() {
            Ok(transaction) => transaction,
            Err(err) => {
                error!("Failed to open database transaction: {}", err);
                return Err("Failed to process block".to_owned());
            }
        };

        for _block in blocks.iter() {
            // TODO: Do stuff here
        }

        if let Err(err) = tx.commit() {
            error!("Failed to commit process block changes: {}", err);
            return Err("Failed to commit process block changes".to_owned());
        };

        let last_block_number = blocks.iter().map(|b| b.number).max();
        if let Some(last_block_number) = last_block_number {
            self.set_last_processed_block_number(last_block_number)?;
        }

        Ok(())
    }
}

impl StateProvider for Database {}

#[cfg(test)]
mod test {}
