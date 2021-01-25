use super::{ILocalStore, LocalEvent};
use rusqlite::{params, types::FromSql, NO_PARAMS};
use rusqlite::{Connection as DB, RowIndex};

/// Implementation of ILocalStore that uses sqlite to
/// persist between restarts
pub struct PersistentLocalStore {
    events: Vec<LocalEvent>,
    db: DB,
}

// TODO: Should we FK to a coin table ?
// TODO: Foreign key to quotes
fn create_tables_if_new(db: &DB) {
    // witnesses
    db.execute(
        "CREATE TABLE IF NOT EXISTS witness (
            coin TEXT NOT NULL,
            txid TEXT NOT NULL,
            quote TEXT NOT NULL,
            tx_block_number INTEGER NOT NULL,
            tx_index INTEGER NOT NULL,
            amount INTEGER NOT NULL,
            PRIMARY KEY (txid, coin)
    )",
        NO_PARAMS,
    )
    .expect("could not create or open DB");
}

// fn read_rows(db: &DB) -> Result<(), String> {
//     let mut stmt = db
//         .prepare("SELECT coin, txid FROM witness;")
//         .expect("Could not prepare stmt");

//     // THIS MAY BE ABLE TO BE CLEANED UP IF USING "PROPER" RELATIONAL DB
//     let res: Vec<Result<(String, String), _>> = stmt
//         .query_map(NO_PARAMS, |row| Ok((row.get(0)?, row.get(1)?)))
//         .map_err(|err| err.to_string())?
//         .collect();

//     println!("the result of the database read rows is: {:#?}", res);
//     Ok(())
// }

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

    fn get_events_as_str(&mut self, last_seen: u64) -> Result<Vec<String>, String> {
        let mut select_witnesses = self
            .db
            .prepare("SELECT * FROM witness")
            .expect("Could not prepare stmt");

        let mut rows = select_witnesses
            .query(NO_PARAMS)
            .map_err(|err| err.to_string())?;
        // .expect("Something went wrong");

        // let val: Result<String, _> = select_witnesses
        //     .query_row(params![], |row| row.get(1))
        //     .map_err(|e| e.to_string());
        // println!("The returned result is: {:#?}", res);
        // for row in res {
        //     println!("Here's a row: {:#?}", row);
        // }
        while let Some(result_row) = rows.next().expect("no next") {
            // let row = try!(result_row);
            // let row = result_row.unwrap();
            let coin: String = result_row.get(0).expect("couldn't get it");
            println!("Result row, 0: {:#?}", coin);
        }
        Err("no".to_string())
    }
}

impl ILocalStore for PersistentLocalStore {
    fn add_events(&mut self, events: Vec<LocalEvent>) -> Result<(), String> {
        // add witnesses
        for event in events {
            match event {
                LocalEvent::Witness(evt) => {
                    match self.db.execute(
                        "
                    INSERT INTO witness
                    (coin, txid, quote, tx_block_number, tx_index, amount)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                    ",
                        // :( why can't we use anything bigger than u32, this is v. bad
                        params![
                            evt.coin.to_string(),
                            evt.transaction_id.to_string(),
                            evt.quote.to_string(),
                            evt.transaction_block_number as u32,
                            evt.transaction_index as u32,
                            evt.amount as u32
                        ],
                    ) {
                        Ok(res) => {
                            trace!(
                                "Witness ({:#}, {:#} added to db",
                                evt.coin,
                                evt.transaction_id
                            )
                        }
                        Err(e) => {
                            println!("could not add witness: {:#?}", e);
                            error!("Witness could not be added to db, {:#?}", e)
                        }
                    }
                }
                _ => {
                    // nothing
                }
            }
        }

        Ok(())
    }

    fn get_events(&mut self, last_seen: u64) -> Option<Vec<LocalEvent>> {
        let mut select_witnesses = self
            .db
            .prepare("SELECT * FROM witness")
            .expect("Could not prepare stmt");

        let val: Result<String, _> = select_witnesses.query_row(params![], |row| row.get(0));
        println!("The returned result is: {:#?}", val.unwrap());
        None
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

        println!("has created the quote");

        // db
        let result = db
            .add_events(vec![evt.clone()])
            .expect("Error adding an event to the database");

        println!("Added evt");

        // Close the database
        drop(db);

        let mut db = PersistentLocalStore::open(temp_file.path());

        // let total_events = db.total_events();
        // let last_events = db.get_events(0).expect("Could not get last events");
        let resp = db.get_events_as_str(0);
        println!("Here's the str events: {:#?}", resp);

        // assert_eq!(evt, last_events[0]);
    }
}
