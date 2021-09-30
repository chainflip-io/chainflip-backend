use std::{collections::HashMap, convert::TryInto, path::Path};

use super::KeyDB;
use kvdb_rocksdb::{Database, DatabaseConfig};
use slog::o;

use crate::{
    logging::COMPONENT_KEY,
    signing::{client::KeygenResultInfo, KeyId},
};

/// Database for keys that uses rocksdb
pub struct PersistentKeyDB {
    /// Rocksdb database instance
    db: Database,
    logger: slog::Logger,
}

impl PersistentKeyDB {
    pub fn new(path: &Path, logger: &slog::Logger) -> Self {
        let config = DatabaseConfig::default();
        // TODO: Update to kvdb 14 and then can pass in &Path
        let db = Database::open(&config, path.to_str().expect("Invalid path"))
            .expect("could not open database");

        PersistentKeyDB {
            db,
            logger: logger.new(o!(COMPONENT_KEY => "PersistentKeyDB")),
        }
    }
}

impl KeyDB for PersistentKeyDB {
    fn update_key(&mut self, key_id: KeyId, keygen_result_info: &KeygenResultInfo) {
        let mut tx = self.db.transaction();

        // TODO: this error should be handled better
        let keygen_result_info_encoded =
            bincode::serialize(keygen_result_info).expect("Could not serialize keygen_result_info");

        tx.put_vec(0, &key_id.0, keygen_result_info_encoded);
    }

    fn load_keys(&self) -> HashMap<KeyId, KeygenResultInfo> {
        self.db
            .iter(0)
            .filter_map(|(key_id, key_info)| {
                let key_id: Vec<u8> = match key_id.try_into() {
                    Ok(key_id) => Some(key_id),
                    Err(err) => {
                        slog::error!(self.logger, "Could not deserialize key_id from DB: {}", err);
                        None
                    }
                }?;

                let key_id: KeyId = KeyId(key_id);
                let key_info_bytes: Vec<u8> = key_info.try_into().unwrap();
                match bincode::deserialize::<KeygenResultInfo>(key_info_bytes.as_ref()) {
                    Ok(keygen_info) => return Some((key_id, keygen_info)),
                    Err(err) => {
                        slog::error!(
                            self.logger,
                            "Could not deserialize key_info (key_id: {:?}) from DB: {}",
                            key_id,
                            err
                        );
                        return None;
                    }
                }
            })
            .collect()
    }
}
