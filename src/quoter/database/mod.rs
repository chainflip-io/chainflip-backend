use super::{BlockProcessor, StateProvider};
use crate::side_chain::SideChainBlock;
use rusqlite;
use rusqlite::params;
use rusqlite::Connection;
use std::str::FromStr;
use std::string::ToString;

mod migration;

/// A database for storing and accessing local state
#[derive(Debug)]
pub struct Database {
    conn: Connection,
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
        Database { conn: connection }
    }

    /// Get key value data
    fn get_data<T>(&self, key: &str) -> Option<T>
    where
        T: FromStr,
    {
        let mut statement = match self.conn.prepare("SELECT value from data WHERE key = ?1;") {
            Ok(statement) => statement,
            Err(_) => return None,
        };
        let string_val: String = match statement.query_row(params![key], |row| row.get(0)) {
            Ok(result) => result,
            Err(_) => return None,
        };

        string_val.parse().ok()
    }

    /// Set key value data
    fn set_data<T>(&self, key: &str, value: Option<T>) -> Result<(), String>
    where
        T: ToString,
    {
        let string_value = value.map(|value| value.to_string());
        match self.conn.execute(
            "INSERT OR REPLACE INTO data (key, value) VALUES (?1, ?2)",
            params![key, string_value],
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
        let tx = match self.conn.transaction() {
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
mod test {
    use super::*;

    fn setup() -> Database {
        let connection = Connection::open_in_memory().expect("Failed to open connection");
        Database::new(connection)
    }

    #[test]
    fn test_set_data_stores_value_correctly() {
        let database = setup();
        let key = "some_data";
        database
            .set_data(key, Some(1))
            .expect("Failed to set data.");

        let mut statement = database
            .conn
            .prepare("SELECT value from data WHERE key = ?1;")
            .unwrap();
        let val: String = statement
            .query_row(params![key], |row| row.get(0))
            .expect("Expected data to be set");

        assert_eq!(val, "1");
    }

    #[test]
    fn test_get_data_should_return_data() {
        let database = setup();

        let key = "number";
        let value = 2;
        database
            .conn
            .execute(
                "INSERT OR REPLACE INTO data (key, value) VALUES (?1, ?2)",
                params![key, value.to_string()],
            )
            .expect("Failed to insert data");

        let returned_value: Option<u32> = database.get_data(key);
        assert_eq!(returned_value, Some(value));
    }

    #[test]
    fn test_get_data_returns_none_if_not_set() {
        let database = setup();
        let returned_value: Option<String> = database.get_data("not_set");
        assert_eq!(returned_value, None);
    }

    #[test]
    fn test_get_data_returns_none_if_invalid_type() {
        let database = setup();

        let key = "number";
        let value = "i_am_a_string";
        database
            .conn
            .execute(
                "INSERT OR REPLACE INTO data (key, value) VALUES (?1, ?2)",
                params![key, value],
            )
            .expect("Failed to insert data");

        let returned_value: Option<u32> = database.get_data(key);
        assert_eq!(returned_value, None);
    }
}
