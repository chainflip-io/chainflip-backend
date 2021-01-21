use super::{ILocalStore, LocalEvent};
use rusqlite::Connection as DB;
use rusqlite::{params, NO_PARAMS};

/// Implementation of ILocalStore that uses sqlite to
/// persist between restarts
pub struct PersistentLocalStore {
    events: Vec<LocalEvent>,
    db: DB,
}

// Should we link to a coin table
fn create_tables_if_new(db: &DB) {
    db.execute(
        "CREATE TABLE IF NOT EXISTS witness (
            coin TEXT NOT NULL,
            txid TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            quote TEXT NOT NULL,
            tx_block_number INTEGER NOT NULL,
            tx_index INTEGER NOT NULL,
            amount INTEGER NOT NULL,
            last_seen INTEGER AUTOINCREMENT,
            PRIMARY KEY (coin, txid),
    )",
        NO_PARAMS,
    )
    .expect("could not create or open DB");
}

fn read_rows(db: &DB) -> Result<(), String> {
    let mut stmt = db
        .prepare("SELECT coin, txid FROM witness;")
        .expect("Could not prepare stmt");

    // THIS MAY BE ABLE TO BE CLEANED UP IF USING "PROPER" RELATIONAL DB
    let res: Vec<Result<(String, String), _>> = stmt
        .query_map(NO_PARAMS, |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|err| err.to_string())?
        .collect();

    println!("the result of the database read rows is: {:#?}", res);
    Ok(())
}

impl PersistentLocalStore {
    /// Create a instance of PersistentLocalStore associated with a database file
    /// with name `file`. The file is created if does not exist. The database tables
    /// are created they don't already exist.
    pub fn open(file: &str) -> Self {
        let db = DB::open(file).expect("Could not open the database");

        create_tables_if_new(&db);

        // TODO: If cannot read db here, panic
        let events: Vec<LocalEvent> = Vec::new();

        PersistentLocalStore { db, events }
    }
}

impl ILocalStore for PersistentLocalStore {
    fn add_events(&mut self, events: Vec<LocalEvent>) -> Result<(), String> {
        todo!();
    }

    fn get_events(&mut self, last_seen: u64) -> Option<Vec<LocalEvent>> {
        todo!();
    }

    fn total_events(&mut self) -> u64 {
        todo!();
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::utils::test_utils::{self, data::TestData};
    use chainflip_common::types::coin::Coin;

    #[test]
    fn test_db_created_successfully() {
        todo!();
    }
}
