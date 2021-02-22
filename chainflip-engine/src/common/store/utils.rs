use super::*;

/// Provides utility methods for a SQLite KVS
pub struct SQLite {}

impl SQLite {
    /// Create the table required for the SQLite kvs.
    ///
    /// Make sure this function is called before using KVS functionality
    pub fn create_kvs_table(connection: &Connection) {
        connection
            .execute(
                "CREATE TABLE IF NOT EXISTS data_kvs (key TEXT PRIMARY KEY, value TEXT);",
                NO_PARAMS,
            )
            .expect("Failed to create tables for key-value storage");
    }

    /// Get the data associated with a key
    pub fn get_data<T: FromStr>(connection: &Connection, key: &str) -> Option<T> {
        let mut statement = match connection.prepare("SELECT value from data_kvs WHERE key = ?1;") {
            Ok(statement) => statement,
            Err(_) => return None,
        };
        let string_val: String = match statement.query_row(params![key], |row| row.get(0)) {
            Ok(result) => result,
            Err(_) => return None,
        };

        string_val.parse().ok()
    }

    /// Set the data
    pub fn set_data<T: ToString>(
        connection: &Connection,
        key: &str,
        value: Option<T>,
    ) -> Result<(), String> {
        let result = match value {
            Some(value) => connection.execute(
                "INSERT OR REPLACE INTO data_kvs (key, value) VALUES (?1, ?2)",
                params![key, value.to_string()],
            ),
            None => connection.execute("DELETE FROM data_kvs WHERE key = ?", params![key]),
        };

        result.map(|_| ()).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_get_and_set_data() {
        let connection = Connection::open_in_memory().expect("Failed to open connection");
        SQLite::create_kvs_table(&connection);

        // Test we can get correctly
        SQLite::set_data(&connection, "number", Some(1)).expect("Failed to set number");
        assert_eq!(SQLite::get_data(&connection, "number"), Some(1));

        // Test unset
        SQLite::set_data::<u32>(&connection, "number", None).expect("Failed to set null number");
        assert_eq!(SQLite::get_data::<u32>(&connection, "number"), None);

        // Test value conversion
        SQLite::set_data(&connection, "string", Some("i_am_a_string"))
            .expect("Failed to set string");
        assert_eq!(SQLite::get_data::<u32>(&connection, "string"), None);
    }
}
