use std::{str::FromStr, u64};

use crate::vault::transactions::memory_provider::{StatusWitnessWrapper, WitnessStatus};

use super::{memory_local_store::NULL_STATUS, EventNumber, ILocalStore, LocalEvent, StorageItem};
use rusqlite::Connection as DB;
use rusqlite::{params, NO_PARAMS};

/// Implementation of ILocalStore that uses sqlite to
/// persist between restarts
pub struct PersistentLocalStore {
    db: DB,
}

fn create_tables_if_new(db: &DB) {
    db.execute(
        "CREATE TABLE IF NOT EXISTS events (
            rowid INTEGER PRIMARY KEY AUTOINCREMENT,
            id TEXT NOT NULL UNIQUE,
            data BLOB NOT NULL,
            status TEXT
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

        // Load the events into memory here

        PersistentLocalStore { db }
    }
}

impl ILocalStore for PersistentLocalStore {
    fn add_events(&mut self, events: Vec<LocalEvent>) -> Result<(), String> {
        for event in events {
            let id = event.unique_id();
            let blob = serde_json::to_string(&event).unwrap();
            let mut select_events = self
                .db
                .prepare("SELECT id FROM events WHERE id = ?")
                .expect("Could not prepare stmt");

            let current_id: Result<String, String> = select_events
                .query_row(params![id as i64], |row| row.get(0))
                .map_err(|err| err.to_string());

            // if the id exists, don't try and add it again
            if current_id.is_ok() {
                trace!(
                    "Event id of {} already exists in the local store",
                    current_id.unwrap()
                );
                return Ok(());
            }
            match self.db.execute(
                "
            INSERT INTO events
            (id, data) VALUES (?1, ?2)
            ",
                params![id as i64, blob],
            ) {
                Ok(_) => {
                    trace!("Event ({:#} added to db", id);
                }
                Err(e) => {
                    return Err(format!("Event {:#} could not be added to db, {:#?}", id, e));
                }
            }
        }
        Ok(())
    }

    fn get_events(&self, last_seen: u64) -> Vec<LocalEvent> {
        let mut select_events = self
            .db
            .prepare("SELECT data, rowid as event_number FROM events WHERE rowid > ?")
            .expect("Could not prepare stmt");

        let mut rows = select_events
            // only u32 or smaller is castable to a SQL type
            .query(params![last_seen as u32])
            .map_err(|err| err.to_string())
            .unwrap();

        let mut recent_events: Vec<LocalEvent> = Vec::new();

        while let Some(row) = rows.next().expect("Rows should be readable") {
            let data_str: String = row.get(0).unwrap();
            let mut l_evt = serde_json::from_str::<LocalEvent>(&data_str).unwrap();
            let row_val: i64 = row.get(1).unwrap();
            l_evt.set_event_number(row_val as u64);
            recent_events.push(l_evt);
        }

        recent_events
    }

    fn total_events(&self) -> u64 {
        let mut total_events = self
            .db
            .prepare("SELECT COUNT(*) FROM events")
            .expect("Could not prepare stmt");

        let count: Result<i64, _> = total_events.query_row(NO_PARAMS, |row| row.get(0));

        count.unwrap() as u64
    }

    fn get_witnesses_status(&self, last_seen: u64) -> Vec<StatusWitnessWrapper> {
        let mut select_events = self
            .db
            .prepare("SELECT data, rowid as event_number, status FROM events WHERE rowid > ?")
            .expect("Could not prepare stmt");

        let mut rows = select_events
            .query(params![last_seen as i64])
            .map_err(|err| err.to_string())
            .unwrap();

        let mut recent_witnesses: Vec<StatusWitnessWrapper> = Vec::new();

        while let Some(row) = rows.next().expect("Rows should be readable") {
            let data_str: String = row.get(0).unwrap();
            let witness = serde_json::from_str::<LocalEvent>(&data_str).unwrap();
            if let LocalEvent::Witness(mut w) = witness {
                let row_val: i64 = row.get(1).unwrap();
                let status: String = row.get(2).unwrap_or(NULL_STATUS.to_string());
                let witness_status: WitnessStatus =
                    WitnessStatus::from_str(&status).unwrap_or(WitnessStatus::AwaitingConfirmation);
                w.event_number = Some(row_val as u64);
                let status_witness_wrapper = StatusWitnessWrapper {
                    inner: w,
                    status: witness_status,
                };
                recent_witnesses.push(status_witness_wrapper);
            }
        }

        recent_witnesses
    }

    fn set_witness_status(&mut self, id: u64, status: WitnessStatus) -> Result<(), String> {
        let status = status.to_string();

        match self.db.execute(
            "
            UPDATE events SET status = ?1
            WHERE id = ?2
            ",
            params![status, id as i64],
        ) {
            Ok(_) => {
                debug!("Witness {} updated to status {}", id, status);
            }
            Err(e) => {
                error!("Failed to update witness {}", id);
                return Err(e.to_string());
            }
        };

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::utils::test_utils::{self, data::TestData};
    use chainflip_common::types::{coin::Coin, unique_id::GetUniqueId};

    #[test]
    fn test_db_created_successfully() {
        let temp_file = test_utils::TempRandomFile::new();

        let mut db = PersistentLocalStore::open(temp_file.path());

        let evt: LocalEvent = TestData::witness(0, 100, Coin::ETH).into();

        db.add_events(vec![evt.clone()])
            .expect("Error adding an event to the database");

        // Close the database
        drop(db);

        let db = PersistentLocalStore::open(temp_file.path());

        let events = db.get_events(0);
        assert_eq!(events.len(), 1);
        let first_evt = events.first().unwrap();

        if let LocalEvent::Witness(w) = first_evt {
            assert_eq!(w.amount, 100);
        };
    }

    #[test]
    fn get_all_events() {
        let temp_file = test_utils::TempRandomFile::new();

        let mut db = PersistentLocalStore::open(temp_file.path());

        let evt: LocalEvent = TestData::witness(0, 100, Coin::ETH).into();
        let evt2: LocalEvent = LocalEvent::DepositQuote(TestData::deposit_quote(Coin::ETH));

        db.add_events(vec![evt.clone(), evt2.clone()])
            .expect("Error adding an event to the database");

        let all_events = db.get_events(0);

        assert_eq!(all_events.len(), 2);
    }

    #[test]
    fn get_events_last_seen_non_zero() {
        let temp_file = test_utils::TempRandomFile::new();

        let mut db = PersistentLocalStore::open(temp_file.path());

        let evt: LocalEvent = TestData::witness(0, 100, Coin::ETH).into();
        let evt2: LocalEvent = LocalEvent::DepositQuote(TestData::deposit_quote(Coin::ETH));

        db.add_events(vec![evt.clone(), evt2.clone()])
            .expect("Error adding an event to the database");

        let all_events = db.get_events(1);

        assert_eq!(all_events.len(), 1);
    }

    #[test]
    fn get_total_events() {
        let temp_file = test_utils::TempRandomFile::new();

        let mut db = PersistentLocalStore::open(temp_file.path());

        let evt: LocalEvent = TestData::witness(0, 100, Coin::ETH).into();
        let evt2: LocalEvent = LocalEvent::DepositQuote(TestData::deposit_quote(Coin::ETH));

        assert_eq!(db.total_events(), 0);

        db.add_events(vec![evt.clone(), evt2.clone()])
            .expect("Error adding an event to the database");

        assert_eq!(db.total_events(), 2);
    }

    #[test]
    fn set_witness_status() {
        let temp_file = test_utils::TempRandomFile::new();

        let mut db = PersistentLocalStore::open(temp_file.path());

        let witness: LocalEvent = TestData::witness(0, 100, Coin::ETH).into();

        // add a witness to the database, without specifying a status
        db.add_events(vec![witness.clone()])
            .expect("Error adding an event to the database");

        assert_eq!(db.total_events(), 1);

        let events = db.get_witnesses_status(0);
        let witness_from_db = events.first().unwrap();
        // ensure the default return is AwaitingConfirmation
        assert_eq!(witness_from_db.status, WitnessStatus::AwaitingConfirmation);

        // Update the witness status
        db.set_witness_status(witness_from_db.inner.unique_id(), WitnessStatus::Confirmed)
            .unwrap();

        let events = db.get_witnesses_status(0);

        let witness_from_db = events.first().unwrap();
        assert_eq!(witness_from_db.status, WitnessStatus::Confirmed);
        assert_eq!(witness_from_db.inner.event_number, Some(1));
    }

    #[test]
    fn attempt_to_add_duplicate_id_entry_is_skipped() {
        let temp_file = test_utils::TempRandomFile::new();

        let mut db = PersistentLocalStore::open(temp_file.path());

        let witness: LocalEvent = TestData::witness(0, 100, Coin::ETH).into();

        // add a witness to the database, without specifying a status
        db.add_events(vec![witness.clone()])
            .expect("Error adding an event to the database");

        assert_eq!(db.total_events(), 1);

        db.add_events(vec![witness.clone()])
            .expect("Error adding duplicate to database");

        // event just gets skipped, so total remains the same
        assert_eq!(db.total_events(), 1);
    }
}
