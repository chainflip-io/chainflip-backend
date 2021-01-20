use super::{ILocalStore, LocalEvent};
use rusqlite::Connection as DB;
use rusqlite::{params, NO_PARAMS};

/// Implementation of ILocalStore that uses sqlite to
/// persist between restarts
pub struct PersistentLocalStore {
    events: Vec<LocalEvent>,
    db: DB,
}

// #[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Serialize, Deserialize)]
// #[serde(rename_all = "camelCase")]
// pub struct Witness {
//     /// A unique identifier
//     pub id: UUIDv4,
//     /// Creation timestamp
//     pub timestamp: Timestamp,
//     /// The identifier of the quote related to the witness.
//     pub quote: UUIDv4,
//     /// The utf-8 encoded bytes of the input transaction id or hash on the actual blockchain
//     pub transaction_id: ByteString,
//     /// The transaction block number on the actual blockchain
//     pub transaction_block_number: u64,
//     /// The input transaction index in the block
//     pub transaction_index: u64,
//     /// The amount that was sent.
//     pub amount: AtomicAmount,
//     /// The type of coin that was sent.
//     pub coin: Coin,
// }


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
            amount INTEGER NOT NULL
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
