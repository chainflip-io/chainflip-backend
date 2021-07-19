use std::{collections::HashMap, convert::TryInto};

use super::KeyDB;
use kvdb_rocksdb::{Database, DatabaseConfig};
use log::error;

use crate::signing::{client::KeygenResultInfo, KeyId};

/// Database for keys that uses rocksdb
pub struct PersistentKeyDB {
    /// Rocksdb database instance
    db: Database,
}

impl PersistentKeyDB {
    pub fn new(path: &str) -> Self {
        let config = DatabaseConfig::default();
        let db = Database::open(&config, path).expect("could not open database");

        PersistentKeyDB { db }
    }
}

impl KeyDB for PersistentKeyDB {
    fn update_key(&mut self, key_id: KeyId, key: &KeygenResultInfo) {
        let mut tx = self.db.transaction();

        let db_key = key_id.0.to_be_bytes();
        // TODO: this error should be handled better
        let key_encoded = bincode::serialize(key).expect("Could not serialize key");

        tx.put_vec(0, &db_key, key_encoded);
    }

    fn load_keys(&self) -> HashMap<KeyId, KeygenResultInfo> {
        self.db
            .iter(0)
            .filter_map(|(key_id, key)| {
                let key_id: &[u8; 8] = match key_id.as_ref().try_into() {
                    Ok(key_id) => Some(key_id),
                    Err(err) => {
                        error!("Could not deserialize key_id from DB: {}", err);
                        None
                    }
                }?;

                let key_id: KeyId = KeyId(u64::from_be_bytes(key_id.clone()));

                let key_info = bincode::deserialize(key.as_ref()).unwrap_or_else(|err| {
                    error!(
                        "Could not deserialize key (key_id: {}) from DB: {}",
                        key_id.0, err
                    );
                    None
                })?;

                Some((key_id, key_info))
            })
            .collect()
    }
}
