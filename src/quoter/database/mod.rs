use super::{BlockProcessor, StateProvider};
use crate::{common::store::utils::SQLite as KVS, side_chain::SideChainBlock};
use rusqlite;
use rusqlite::Connection;

mod migration;

/// A database for storing and accessing local state
#[derive(Debug)]
pub struct Database {
    connection: Connection,
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
        KVS::create_kvs_table(&connection);
        Database { connection }
    }

    fn set_last_processed_block_number(&self, block_number: u32) -> Result<(), String> {
        KVS::set_data(
            &self.connection,
            "last_processed_block_number",
            Some(block_number),
        )
    }
}

impl BlockProcessor for Database {
    fn get_last_processed_block_number(&self) -> Option<u32> {
        KVS::get_data(&self.connection, "last_processed_block_number")
    }

    fn process_blocks(&mut self, blocks: Vec<SideChainBlock>) -> Result<(), String> {
        let tx = match self.connection.transaction() {
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

        let last_block_number = blocks.iter().map(|b| b.id).max();
        if let Some(last_block_number) = last_block_number {
            self.set_last_processed_block_number(last_block_number)?;
        }

        Ok(())
    }
}

impl StateProvider for Database {}

#[cfg(test)]
mod test {
    use super::*;

    fn _setup() -> Database {
        let connection = Connection::open_in_memory().expect("Failed to open connection");
        Database::new(connection)
    }
}
