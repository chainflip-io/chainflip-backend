use rusqlite::{Connection, Transaction};
use rusqlite::{Error, NO_PARAMS};

/// Migrate a database
///
/// # Panics
///
/// panics if any error occurs while migrating.
pub fn migrate_database(connection: &mut Connection) {
    loop {
        let database_version: u32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("Failed to query user_version of database");

        let next_version = database_version + 1;
        match next_version {
            // Version 1 will always be the start
            1 => migrate_to_version_1(connection),
            _ => break,
        }
    }
}

/// Apply database changes
///
/// This will automatically update the `user_version` of the database to the `version` specified after changes are made.
fn apply_changes<F>(connection: &mut Connection, version: u32, changes: F) -> Result<(), Error>
where
    F: FnOnce(&Transaction) -> Result<(), Error>,
{
    info!("Migrating database to version {}", version);
    let tx = connection.transaction()?;
    changes(&tx)?;
    tx.pragma_update(None, "user_version", &version)?;
    tx.commit()?;
    info!("Migrated succefully");

    Ok(())
}

fn migrate_to_version_1(connection: &mut Connection) {
    apply_changes(connection, 1, |tx| {
        tx.execute(
            "CREATE TABLE IF NOT EXISTS events (
            id char(36) PRIMARY KEY,
            type TEXT NOT NULL,
            data BLOB NOT NULL
            )",
            NO_PARAMS,
        )
        .expect("could not create or open DB");

        Ok(())
    })
    .expect("Failed to migrate database to version 1");
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_migrate_database_should_update_to_latest_version() {
        let latest_version = 1;

        let mut connection =
            Connection::open_in_memory().expect("Expected connection to be opened.");
        migrate_database(&mut connection);

        let database_version: u32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("Failed to query user_version.");

        assert_eq!(database_version, latest_version);
    }
}
