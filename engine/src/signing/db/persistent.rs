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
    pub db: Database,
    logger: slog::Logger,
}

impl PersistentKeyDB {
    pub fn new(path: &Path, logger: &slog::Logger) -> Self {
        let config = DatabaseConfig::default();
        // TODO: Update to kvdb 14 and then can pass in &Path

        // TODO: Error report this path with the fully qualified path it's trying to access.
        // LOG the path it's accessing even if it doesn't error
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
        // commit the tx to the database
        self.db.write(tx).expect(&format!(
            "Could not write key share for key_id `{:?}` to database",
            key_id
        ));
    }

    fn load_keys(&self) -> HashMap<KeyId, KeygenResultInfo> {
        self.db
            .iter(0)
            .filter_map(|(key_id, key)| {
                let key_id: Vec<u8> = match key_id.try_into() {
                    Ok(key_id) => Some(key_id),
                    Err(err) => {
                        slog::error!(self.logger, "Could not deserialize key_id from DB: {}", err);
                        None
                    }
                }?;

                let key_id: KeyId = KeyId(key_id);

                let key_info = bincode::deserialize(key.as_ref()).unwrap_or_else(|err| {
                    slog::error!(
                        self.logger,
                        "Could not deserialize key (key_id: {:?}) from DB: {}",
                        key_id,
                        err
                    );
                    None
                })?;

                Some((key_id, key_info))
            })
            .collect()
    }
}

// TODO: WRITE TESTS
