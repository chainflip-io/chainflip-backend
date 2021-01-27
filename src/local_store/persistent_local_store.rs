use super::{ILocalStore, LocalEvent, StorageItem};
use rusqlite::Connection as DB;
use rusqlite::{params, NO_PARAMS};

/// Implementation of ILocalStore that uses sqlite to
/// persist between restarts
pub struct PersistentLocalStore {
    // is this required?
    events: Vec<LocalEvent>,
    db: DB,
}

fn create_tables_if_new(db: &DB) {
    db.execute(
        "CREATE TABLE IF NOT EXISTS events (
                id TEXT PRIMARY KEY,
                data BLOB NOT NULL
    )",
        NO_PARAMS,
    )
    .expect("could not create or open DB");
}

impl PersistentLocalStore {
    /// Create a instance of PersistentLocalStore associated with a database file
    /// with name `file`. The file is created if does not exist. The database tables
    /// are created they don't already exist.
    pub fn open(file: &str) -> Self {
        let db = DB::open(file).expect("Could not open the database");

        create_tables_if_new(&db);

        println!("Could create the tables");

        // TODO: If cannot read db here, panic
        // Not sure if events even necessary here
        let events: Vec<LocalEvent> = Vec::new();

        PersistentLocalStore { db, events }
    }
}

impl ILocalStore for PersistentLocalStore {
    fn add_events(&mut self, events: Vec<LocalEvent>) -> Result<(), String> {
        // add witnesses
        for event in events {
            let id = event.unique_id();
            let blob = serde_json::to_string(&event).unwrap();
            match self.db.execute(
                "
            INSERT INTO events
            (id, data) VALUES (?1, ?2)
            ",
                params![id, blob],
            ) {
                Ok(_) => {
                    trace!("Witness ({:#} added to db", id);
                }
                Err(e) => {
                    println!("Witness {:#} could not be added to db, {:#?}", id, e);
                    return Err(format!(
                        "Witness {:#} could not be added to db, {:#?}",
                        id, e
                    ));
                }
            }
        }
        Ok(())
    }

    // TODO: Implement with > last_seen
    fn get_events(&mut self, last_seen: u64) -> Option<Vec<LocalEvent>> {
        let mut select_events = self
            .db
            .prepare("SELECT data FROM events")
            .expect("Could not prepare stmt");

        // add row_id > last_seen here
        let mut rows = select_events
            .query(NO_PARAMS)
            .map_err(|err| err.to_string())
            .unwrap();

        let mut recent_events: Vec<LocalEvent> = Vec::new();
        for row in rows.next() {
            match row {
                Some(evt) => {
                    let str_val: String = evt.get(0).unwrap();
                    let l_evt = serde_json::from_str::<LocalEvent>(&str_val).unwrap();
                    recent_events.push(l_evt);
                }
                None => {
                    println!("Nothing to see here");
                    return None;
                }
            }
        }
        Some(recent_events)
    }

    // Do we really neeed this?
    fn total_events(&mut self) -> u64 {
        return 0;
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::utils::test_utils::{self, data::TestData};
    use chainflip_common::types::{coin::Coin, UUIDv4};

    #[test]
    fn test_db_created_successfully() {
        let temp_file = test_utils::TempRandomFile::new();

        let mut db = PersistentLocalStore::open(temp_file.path());

        let evt: LocalEvent = TestData::witness(UUIDv4::new(), 100, Coin::ETH).into();

        db.add_events(vec![evt.clone()])
            .expect("Error adding an event to the database");

        // Close the database
        drop(db);

        let mut db = PersistentLocalStore::open(temp_file.path());

        let events = db.get_events(0).unwrap();
        assert_eq!(events.len(), 1);
        let first_evt = events.first().unwrap();

        if let LocalEvent::Witness(w) = first_evt {
            assert_eq!(w.amount, 100);
        };
    }
}
