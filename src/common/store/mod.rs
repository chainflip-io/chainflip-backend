use rusqlite::{params, Connection, NO_PARAMS};
use std::str::FromStr;

/// Utility functions for the store
pub mod utils;
use utils::SQLite;

/// An interface for key value stores
///
/// Call `create_kvs_tables()` to setup the correct tables
pub(crate) trait KeyValueStore {
    /// Get the data associated with a key
    fn get_data<T: FromStr>(&self, key: &str) -> Option<T>;

    /// Set the data
    fn set_data<T: ToString>(&self, key: &str, value: Option<T>) -> Result<(), String>;
}

/// A key value store which uses SQLite
#[derive(Debug)]
pub struct PersistentKVS {
    connection: Connection,
}

impl PersistentKVS {
    /// Create a new SQLite key value store from a connection.
    ///
    /// This will add tables into the database if they don't exist.
    pub fn new(connection: Connection) -> Self {
        SQLite::create_kvs_table(&connection);
        PersistentKVS { connection }
    }
}

impl KeyValueStore for PersistentKVS {
    fn get_data<T: FromStr>(&self, key: &str) -> Option<T> {
        SQLite::get_data(&self.connection, key)
    }

    fn set_data<T: ToString>(&self, key: &str, value: Option<T>) -> Result<(), String> {
        SQLite::set_data(&self.connection, key, value)
    }
}
