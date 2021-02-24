use self::types::TransactionType;
use super::{EventProcessor, StateProvider};
use crate::{
    common::store::utils::SQLite as KVS,
    common::{Liquidity, LiquidityProvider, PoolCoin},
    local_store::LocalEvent,
};
use chainflip_common::types::{chain::*, unique_id::GetUniqueId};
use itertools::Itertools;
use rusqlite::{self, Row, ToSql, Transaction};
use rusqlite::{params, Connection};
use serde::de::DeserializeOwned;
use std::collections::HashMap;

mod migration;
mod types;

/// A database for storing and accessing local state
#[derive(Debug)]
pub struct Database {
    connection: Connection,
}

impl Database {
    /// Returns a database instance from the given path.
    pub fn open(file: &str) -> Self {
        let connection = Connection::open(file).expect("Could not open the database");
        Database::new(connection)
    }

    /// Returns a database instance with the given connection.
    pub fn new(mut connection: Connection) -> Self {
        migration::migrate_database(&mut connection);
        KVS::create_kvs_table(&connection);
        Database { connection }
    }

    fn increment_last_processed_event_number(&self, num_events: u64) -> Result<(), String> {
        let last_event_num = self.get_last_processed_event_number().unwrap_or(0);
        let new_last_event_num = last_event_num
            .checked_add(num_events)
            .expect("overflow on last_processsed_event_num");
        KVS::set_data(
            &self.connection,
            "last_processed_event_number",
            Some(new_last_event_num),
        )
    }

    fn process_events(db: &Transaction, txs: &[LocalEvent]) {
        for tx in txs {
            match tx {
                LocalEvent::Witness(tx) => {
                    let serialized = serde_json::to_string(tx).unwrap();
                    Database::insert_transaction(db, tx.unique_id(), tx.into(), serialized)
                }
                LocalEvent::PoolChange(tx) => {
                    let serialized = serde_json::to_string(tx).unwrap();
                    Database::insert_transaction(db, tx.unique_id(), tx.into(), serialized)
                }
                LocalEvent::SwapQuote(tx) => {
                    let serialized = serde_json::to_string(tx).unwrap();
                    Database::insert_transaction(db, tx.unique_id(), tx.into(), serialized)
                }
                LocalEvent::DepositQuote(tx) => {
                    let serialized = serde_json::to_string(tx).unwrap();
                    Database::insert_transaction(db, tx.unique_id(), tx.into(), serialized)
                }
                LocalEvent::Output(tx) => {
                    let serialized = serde_json::to_string(tx).unwrap();
                    Database::insert_transaction(db, tx.unique_id(), tx.into(), serialized)
                }
                LocalEvent::OutputSent(tx) => {
                    let serialized = serde_json::to_string(tx).unwrap();
                    Database::insert_transaction(db, tx.unique_id(), tx.into(), serialized)
                }
                LocalEvent::Deposit(tx) => {
                    let serialized = serde_json::to_string(tx).unwrap();
                    Database::insert_transaction(db, tx.unique_id(), tx.into(), serialized)
                }
                LocalEvent::WithdrawRequest(tx) => {
                    let serialized = serde_json::to_string(tx).unwrap();
                    Database::insert_transaction(db, tx.unique_id(), tx.into(), serialized)
                }
                LocalEvent::Withdraw(tx) => {
                    let serialized = serde_json::to_string(tx).unwrap();
                    Database::insert_transaction(db, tx.unique_id(), tx.into(), serialized)
                }
            }
        }
    }

    fn insert_transaction(
        db: &Transaction,
        uuid: UniqueId,
        tx_type: TransactionType,
        data: String,
    ) {
        db.execute(
            "INSERT OR REPLACE INTO events (id, type, data) VALUES (?1, ?2, ?3)",
            params![uuid.to_string(), tx_type.to_string(), data],
        )
        .expect("Failed to create statement");
    }

    fn get_rows<T, P, F>(&self, stmt: &str, params: P, row_fn: F) -> Vec<T>
    where
        T: DeserializeOwned,
        P: IntoIterator,
        P::Item: ToSql,
        F: FnMut(&Row<'_>) -> Result<String, rusqlite::Error>,
    {
        let mut stmt = self
            .connection
            .prepare(stmt)
            .expect("Could not prepare stmt");

        let rows = match stmt.query_map(params, row_fn) {
            Ok(rows) => rows,
            Err(err) => {
                debug!("Failed to fetch database rows: {}", err);
                return vec![];
            }
        };

        let mut results = vec![];
        for result in rows {
            if let Some(data) = result
                .ok()
                .and_then(|data| serde_json::from_str::<T>(&data).ok())
            {
                results.push(data)
            }
        }

        results
    }

    fn get_row<T, P, F>(&self, stmt: &str, params: P, row_fn: F) -> Option<T>
    where
        T: DeserializeOwned,
        P: IntoIterator,
        P::Item: ToSql,
        F: FnOnce(&Row<'_>) -> Result<String, rusqlite::Error>,
    {
        let mut stmt = self
            .connection
            .prepare(stmt)
            .expect("Could not prepare stmt");

        let data = stmt.query_row(params, row_fn).ok()?;
        serde_json::from_str::<T>(&data).ok()
    }

    fn get_transactions<T: DeserializeOwned>(&self, tx_type: TransactionType) -> Vec<T> {
        self.get_rows(
            "SELECT data from events where type = ?",
            params![tx_type.to_string()],
            |row| row.get(0),
        )
    }

    fn get_transaction<T: DeserializeOwned>(&self, id: UniqueId) -> Option<T> {
        self.get_row(
            "SELECT data from events where id = ?",
            params![id.to_string()],
            |row| row.get(0),
        )
    }
}

impl EventProcessor for Database {
    fn get_last_processed_event_number(&self) -> Option<u64> {
        KVS::get_data(&self.connection, "last_processed_event_number")
    }

    fn process_events(&mut self, events: &[LocalEvent]) -> Result<(), String> {
        let conn = match self.connection.transaction() {
            Ok(tx) => tx,
            Err(err) => {
                error!("Failed to open database transaction: {}", err);
                return Err("Failed to process block".to_owned());
            }
        };

        Database::process_events(&conn, events);

        if let Err(err) = conn.commit() {
            error!("Failed to commit process events changes: {}", err);
            return Err(format!("Failed to commit process events changes: {}", err));
        }

        if let Err(err) = self.increment_last_processed_event_number(events.len() as u64) {
            error!("Failed to increment last_processed_event_number: {}", err);
            return Err(format!("Failed to increment last_processed_event_number"));
        }

        Ok(())
    }
}

impl StateProvider for Database {
    fn get_swap_quotes(&self) -> Vec<SwapQuote> {
        self.get_transactions(TransactionType::SwapQuote)
    }

    fn get_swap_quote(&self, id: UniqueId) -> Option<SwapQuote> {
        self.get_transaction(id)
    }

    fn get_deposit_quotes(&self) -> Vec<DepositQuote> {
        self.get_transactions(TransactionType::DepositQuote)
    }

    fn get_deposit_quote(&self, id: UniqueId) -> Option<DepositQuote> {
        self.get_transaction(id)
    }

    fn get_witnesses(&self) -> Vec<Witness> {
        self.get_transactions(TransactionType::Witness)
    }

    fn get_outputs(&self) -> Vec<Output> {
        self.get_transactions(TransactionType::Output)
    }

    fn get_output_sents(&self) -> Vec<OutputSent> {
        self.get_transactions(TransactionType::Sent)
    }

    fn get_deposits(&self) -> Vec<Deposit> {
        self.get_transactions(TransactionType::Deposit)
    }

    fn get_withdraws(&self) -> Vec<Withdraw> {
        self.get_transactions(TransactionType::Withdraw)
    }

    fn get_withdraw_requests(&self) -> Vec<WithdrawRequest> {
        self.get_transactions(TransactionType::WithdrawRequest)
    }

    fn get_pools(&self) -> std::collections::HashMap<PoolCoin, Liquidity> {
        let mut map = HashMap::new();
        let changes: Vec<PoolChange> = self.get_transactions(TransactionType::PoolChange);

        let groups = changes
            .iter()
            .map(|tx| (PoolCoin::from(tx.pool).unwrap(), tx))
            .into_group_map();
        for (coin, txs) in groups {
            let mut liquidity = Liquidity::zero();
            for pool_change in txs {
                let depth = liquidity.depth as i128 + pool_change.depth_change;
                let base_depth = liquidity.base_depth as i128 + pool_change.base_depth_change;
                if depth < 0 || base_depth < 0 {
                    panic!("Negative liquidity depth found")
                }
                liquidity.depth = depth as u128;
                liquidity.base_depth = base_depth as u128;
            }

            map.insert(coin, liquidity);
        }

        map
    }
}

impl LiquidityProvider for Database {
    fn get_liquidity(&self, pool: PoolCoin) -> Option<Liquidity> {
        self.get_pools().get(&pool).cloned()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        common::*,
        utils::test_utils::{data::TestData, staking::get_random_staker},
    };
    use chainflip_common::types::coin::Coin;
    use rusqlite::NO_PARAMS;

    fn setup() -> Database {
        let connection = Connection::open_in_memory().expect("Failed to open connection");
        Database::new(connection)
    }

    struct RawData {
        id: String,
        data: String,
    }

    #[test]
    fn inserts_transaction() {
        let mut db = setup();
        let tx = db.connection.transaction().unwrap();

        Database::insert_transaction(&tx, 0, TransactionType::PoolChange, "Hello".into());

        tx.commit().unwrap();

        let results = db
            .connection
            .query_row("select id, data from events", NO_PARAMS, |row| {
                Ok(RawData {
                    id: row.get(0).unwrap(),
                    data: row.get(1).unwrap(),
                })
            })
            .unwrap();

        assert_eq!(results.id, "0");
        assert_eq!(&results.data, "Hello");
    }

    #[test]
    fn processes_events() {
        let mut db = setup();

        assert!(db.get_last_processed_event_number().is_none());

        let events: Vec<LocalEvent> = vec![
            TestData::pool_change(Coin::BTC, -100, 100).into(),
            TestData::swap_quote(Coin::ETH, Coin::OXEN).into(),
            TestData::deposit_quote(Coin::ETH).into(),
        ];

        db.process_events(&events).unwrap();

        assert_eq!(db.get_last_processed_event_number(), Some(3));
    }

    #[test]
    fn processes_transactions() {
        let mut db = setup();
        let tx = db.connection.transaction().unwrap();
        let staker = get_random_staker();

        let events: Vec<LocalEvent> = vec![
            TestData::pool_change(Coin::BTC, -100, 100).into(),
            TestData::swap_quote(Coin::ETH, Coin::OXEN).into(),
            TestData::deposit_quote(Coin::ETH).into(),
            TestData::witness(1212, 100, Coin::ETH).into(),
            TestData::output(Coin::ETH, 100).into(),
            TestData::output_sent(Coin::ETH).into(),
            TestData::withdraw_request_for_staker(&staker, Coin::ETH).into(),
        ];

        Database::process_events(&tx, &events);

        tx.commit().expect("Expected events to be added");

        let count: u32 = db
            .connection
            .query_row("SELECT COUNT(*) from events", NO_PARAMS, |r| r.get(0))
            .unwrap();

        assert_eq!(count, 7);
    }

    #[test]
    fn returns_pools() {
        let mut db = setup();
        let events: Vec<LocalEvent> = vec![
            TestData::pool_change(Coin::BTC, 100, 100).into(),
            TestData::pool_change(Coin::ETH, 75, 75).into(),
            TestData::pool_change(Coin::BTC, 100, -50).into(),
            TestData::pool_change(Coin::BTC, 0, -50).into(),
        ];

        db.process_events(&events).unwrap();

        let pools = db.get_pools();

        let btc_pool = pools.get(&PoolCoin::BTC).unwrap();
        assert_eq!(btc_pool.depth, 200);
        assert_eq!(btc_pool.base_depth, 0);

        let eth_pool = pools.get(&PoolCoin::ETH).unwrap();
        assert_eq!(eth_pool.depth, 75);
        assert_eq!(eth_pool.base_depth, 75);
    }
}
