use crate::quoter::database::Database;
use rusqlite::Connection;

pub fn setup_memory_db() -> Database {
    let connection = Connection::open_in_memory().expect("Failed to open connection");
    Database::new(connection)
}
