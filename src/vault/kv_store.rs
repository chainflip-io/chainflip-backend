use rusqlite::{params, Connection, NO_PARAMS};
use std::str::FromStr;

/// An interface for key value stores
pub trait KeyValueStore {
    /// Get the data associated with a key
    fn get_data<T: FromStr>(&self, key: &str) -> Option<T>;
    /// Set the data
    fn set_data<T: ToString>(&self, key: &str, value: Option<T>) -> Result<(), String>;
}

fn create_tables_if_needed(connection: &Connection) {
    connection
        .execute(
            "CREATE TABLE IF NOT EXISTS data (key TEXT PRIMARY KEY, value TEXT);",
            NO_PARAMS,
        )
        .expect("Failed to create tables for key-value storage");
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
        create_tables_if_needed(&connection);
        PersistentKVS { connection }
    }
}

impl KeyValueStore for PersistentKVS {
    fn get_data<T: FromStr>(&self, key: &str) -> Option<T> {
        let mut statement = match self
            .connection
            .prepare("SELECT value from data WHERE key = ?1;")
        {
            Ok(statement) => statement,
            Err(_) => return None,
        };
        let string_val: String = match statement.query_row(params![key], |row| row.get(0)) {
            Ok(result) => result,
            Err(_) => return None,
        };

        string_val.parse().ok()
    }

    fn set_data<T: ToString>(&self, key: &str, value: Option<T>) -> Result<(), String> {
        let result = match value {
            Some(value) => self.connection.execute(
                "INSERT OR REPLACE INTO data (key, value) VALUES (?1, ?2)",
                params![key, value.to_string()],
            ),
            None => self
                .connection
                .execute("DELETE FROM data WHERE key = ?", params![key]),
        };

        result.map(|_| ()).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn setup() -> PersistentKVS {
        let connection = Connection::open_in_memory().expect("Failed to open connection");
        PersistentKVS::new(connection)
    }

    #[test]
    fn test_get_and_set_data() {
        let database = setup();

        // Test we can get correctly
        database
            .set_data("number", Some(1))
            .expect("Failed to set number");
        assert_eq!(database.get_data("number"), Some(1));

        // Test unset
        database
            .set_data::<u32>("number", None)
            .expect("Failed to set null number");
        assert_eq!(database.get_data::<u32>("number"), None);

        // Test value conversion
        database
            .set_data("string", Some("i_am_a_string"))
            .expect("Failed to set string");
        assert_eq!(database.get_data::<u32>("string"), None);
    }
}
