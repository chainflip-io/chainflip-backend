use std::{collections::HashMap, convert::TryInto, path::Path};

use super::KeyDB;
use rocksdb::{IteratorMode, Options, DB};
use slog::o;

use crate::{
    logging::COMPONENT_KEY,
    multisig::{client::KeygenResultInfo, KeyId},
};

use anyhow::{Context, Result};

/// This is the version of the data on this current branch
/// This version *must* be bumped, and appropriate migrations
/// written on any changes to the persistent application data format
const DATA_VERSION: u32 = 0;

/// Column for any metadata
pub const METADATA_COL: usize = 1;
pub const DATA_VERSION_KEY: &[u8; 12] = b"data_version";

/// Column for all of the KeygenResultInfo
pub const KEYGEN_DATA_COL: usize = 0;

/// Column family names
const COLUMN_FAMILY: &'static [&'static str] = &["col0", "col1"];

/// Database for keys and persistent metadata
pub struct PersistentKeyDB {
    /// Rocksdb database instance
    db: DB,
    logger: slog::Logger,
}
impl PersistentKeyDB {
    pub fn new(path: &Path, logger: &slog::Logger) -> Result<Self> {
        let logger = logger.new(o!(COMPONENT_KEY => "PersistentKeyDB"));
        let db = if path.exists() {
            // Check the version against the existing database, assuming 0 if no version exists
            let mut opts = Options::default();
            opts.create_missing_column_families(true);
            // We have already check that the database exists
            opts.create_if_missing(false);

            // Open the db
            let cfs = vec![COLUMN_FAMILY[KEYGEN_DATA_COL], COLUMN_FAMILY[METADATA_COL]];
            let db = DB::open_cf(&opts, &path, cfs)
                .map_err(anyhow::Error::msg)
                .context(format!("Failed to open database at: {}", path.display()))?;
            let cf = db
                .cf_handle(COLUMN_FAMILY[METADATA_COL])
                .expect(&format!("Should handle cf {}", COLUMN_FAMILY[METADATA_COL]));

            // Get version number
            let data_version = match db
                .get_cf(&cf, DATA_VERSION_KEY)
                .map_err(anyhow::Error::msg)
                .context("Failed querying for data_version")?
            {
                Some(version) => {
                    let version: [u8; 4] = version.try_into().expect("Version should be a u32");
                    let version = u32::from_be_bytes(version);
                    slog::info!(logger, "Found data_version of {}", version);
                    version
                }
                // If we can't find a data_version, we assume it's the first one
                None => {
                    slog::info!(logger, "Did not find data_version in existing database. Assuming data_version of 0");
                    0
                }
            };

            if data_version != DATA_VERSION {
                slog::error!(logger, "Please perform the required data migrations. Your database has data version: {} but this CFE version uses data version: {}", data_version, DATA_VERSION);
                return Err(anyhow::Error::msg(
                    "Invalid data version on database. Migrations required",
                ));
            }
            db
        } else {
            // Create a new db, setting the version number to the latest version
            let mut opts = Options::default();
            opts.create_missing_column_families(true);
            opts.create_if_missing(true);
            let cfs = vec![COLUMN_FAMILY[KEYGEN_DATA_COL], COLUMN_FAMILY[METADATA_COL]];
            let db = DB::open_cf(&opts, &path, cfs).unwrap();
            let cf = db.cf_handle(COLUMN_FAMILY[METADATA_COL]).expect(&format!(
                "Should get cf handle {}",
                COLUMN_FAMILY[METADATA_COL]
            ));

            // Put the latest version in there
            slog::debug!(logger, "Writing data version: {}", DATA_VERSION);
            match db.put_cf(&cf, DATA_VERSION_KEY, DATA_VERSION.to_be_bytes()) {
                Ok(()) => (),
                Err(error) => {
                    slog::error!(
                        logger,
                        "Failed to add data_version to database on initialisation with error: {:?}. Deleting bad database file...",
                        error
                    );
                    std::fs::remove_dir_all(path)
                        .expect("Should delete bad database initialisation");
                    return Err(anyhow::Error::msg("Failed to initialise database"));
                }
            };
            db
        };

        Ok(PersistentKeyDB { db, logger })
    }
}

impl KeyDB for PersistentKeyDB {
    fn update_key(&mut self, key_id: &KeyId, keygen_result_info: &KeygenResultInfo) {
        // // TODO: this error should be handled better
        let keygen_result_info_encoded =
            bincode::serialize(keygen_result_info).expect("Could not serialize keygen_result_info");

        let cf = self
            .db
            .cf_handle(COLUMN_FAMILY[KEYGEN_DATA_COL])
            .expect(&format!(
                "Should handle cf {}",
                COLUMN_FAMILY[KEYGEN_DATA_COL]
            ));

        self.db
            .put_cf(cf, &key_id.0, &keygen_result_info_encoded)
            .unwrap_or_else(|e| {
                panic!(
                    "Could not write key share for key_id `{}` to database: {}",
                    &key_id, e,
                )
            });
    }

    fn load_keys(&self) -> HashMap<KeyId, KeygenResultInfo> {
        let cf = self
            .db
            .cf_handle(COLUMN_FAMILY[KEYGEN_DATA_COL])
            .expect(&format!(
                "Should get cf handle {}",
                COLUMN_FAMILY[KEYGEN_DATA_COL]
            ));

        self.db
            .iterator_cf(cf, IteratorMode::Start)
            .filter_map(|(key_id, key_info)| {
                let key_id: KeyId = KeyId(key_id.into());
                match bincode::deserialize::<KeygenResultInfo>(&*key_info) {
                    Ok(keygen_info) => {
                        slog::info!(
                            self.logger,
                            "Loaded key_info (key_id: {}) from database",
                            key_id
                        );
                        Some((key_id, keygen_info))
                    }
                    Err(err) => {
                        slog::error!(
                            self.logger,
                            "Could not deserialize key_info (key_id: {}) from database: {}",
                            key_id,
                            err
                        );
                        None
                    }
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    use crate::{
        logging::test_utils::new_test_logger, multisig::db::PersistentKeyDB, testing::assert_ok,
    };

    fn open_db_and_write_version_data(path: &Path, version_data: u32) {
        let cfs = vec![COLUMN_FAMILY[KEYGEN_DATA_COL], COLUMN_FAMILY[METADATA_COL]];
        let mut opts = Options::default();
        opts.create_missing_column_families(true);
        opts.create_if_missing(true);
        let db = DB::open_cf(&opts, &path, cfs).expect("Should open db file");
        let cf = db
            .cf_handle(COLUMN_FAMILY[METADATA_COL])
            .expect("Should get cf handle");

        // write the version data
        db.put_cf(&cf, DATA_VERSION_KEY, version_data.to_be_bytes())
            .expect("Should write DATA_VERSION");
    }

    // To generate this, you can use the test in engine/src/signing/client/client_inner/genesis.rs
    const KEYGEN_RESULT_INFO_HEX: &'static str = "21000000000000000356815a968986af7dd8f84c365429435fba940a8b854129e78739d6d5a5ba74222000000000000000a0687cf58d7838802724b5a0ce902b421605488990c2a1156833743c68cc792303000000000000002100000000000000027cf4fe1aabd5862729d8f96ab07cf175f058fc7b4f79f3fd4fc4f9fba399dbb42100000000000000030bf033482c62d78902ff482b625dd99f025fcd429689123495bd5c5c6224cfda210000000000000002ee6ff7fd3bad3942708e965e728d8923784d36eb57f09d23aa75d8743a27c59b030000000000000030000000000000003547653178463155334555674b6947596a4c43576d6763444858516e66474e45756a775859546a5368463647636d595a0300000000000000300000000000000035444a565645595044465a6a6a394a744a5245327647767065536e7a42415541373456585053706b474b684a5348624e010000000000000030000000000000003546396f664342574c4d46586f747970587462556e624c586b4d315a39417334374752684444464a4473784b6770427502000000000000000300000000000000300000000000000035444a565645595044465a6a6a394a744a5245327647767065536e7a42415541373456585053706b474b684a5348624e30000000000000003546396f664342574c4d46586f747970587462556e624c586b4d315a39417334374752684444464a4473784b6770427530000000000000003547653178463155334555674b6947596a4c43576d6763444858516e66474e45756a775859546a5368463647636d595a03000000000000000100000000000000";

    #[test]
    fn new_db_creates_new_db_with_latest_version_when_db_does_not_exist() {
        let db_path = Path::new("db3");
        let logger = new_test_logger();

        // Create a fresh db, this will write the version data
        let _ = std::fs::remove_dir_all(db_path);
        assert!(!db_path.exists());
        {
            let _ = PersistentKeyDB::new(&db_path, &logger);
        }

        {
            // Open the db file manually
            let cfs = vec![COLUMN_FAMILY[KEYGEN_DATA_COL], COLUMN_FAMILY[METADATA_COL]];
            let db = DB::open_cf(&Options::default(), &db_path, cfs).expect("Should open db file");
            let cf = db
                .cf_handle(COLUMN_FAMILY[METADATA_COL])
                .expect(&format!("Should handle cf {}", COLUMN_FAMILY[METADATA_COL]));

            // Get version number
            let data_version = db
                .get_cf(&cf, DATA_VERSION_KEY)
                .expect("Should get from metadata cf")
                .expect("No version data found");
            assert_eq!(data_version, DATA_VERSION.to_be_bytes());
        }

        // clean up
        std::fs::remove_dir_all(db_path).unwrap();
    }

    #[test]
    fn new_db_returns_db_when_db_data_version_is_latest() {
        let db_path = Path::new("db4");
        let _ = std::fs::remove_dir_all(db_path);

        {
            open_db_and_write_version_data(&db_path, DATA_VERSION);
            assert_ok!(PersistentKeyDB::new(&db_path, &new_test_logger()));
        }

        // clean up
        std::fs::remove_dir_all(db_path).unwrap();
    }

    #[test]
    fn new_db_errors_about_migrations_when_data_version_mismatch() {
        let db_path = Path::new("db5");
        let _ = std::fs::remove_dir_all(db_path);

        {
            // Put a version number that is not the latest version number in the db
            open_db_and_write_version_data(&db_path, DATA_VERSION + 1);
            assert!(PersistentKeyDB::new(&db_path, &new_test_logger()).is_err());
        }

        // clean up
        std::fs::remove_dir_all(db_path).unwrap();
    }

    #[test]
    fn can_open_db_with_no_data_version() {
        let db_path = Path::new("db6");
        let _ = std::fs::remove_dir_all(db_path);

        // Backwards compatibility check
        assert_eq!(
            COLUMN_FAMILY[KEYGEN_DATA_COL], "col0",
            "The keygen data column must be `col0` to be backwards compatible with kvdb_rocksdb"
        );

        {
            // Create the db with no metadata column
            let mut opts = Options::default();
            opts.create_missing_column_families(true);
            opts.create_if_missing(true);
            let _ = DB::open_cf(&opts, &db_path, vec![COLUMN_FAMILY[KEYGEN_DATA_COL]])
                .expect("Should open db file");
        }

        {
            // TODO: check that the data version is assumed to be 0
            assert_ok!(PersistentKeyDB::new(&db_path, &new_test_logger()));
        }

        // clean up
        std::fs::remove_dir_all(db_path).unwrap();
    }

    #[test]
    fn can_new_database() {
        let logger = new_test_logger();
        let db_path = Path::new("db_new");

        {
            let _ = PersistentKeyDB::new(&db_path, &logger).unwrap();
        }

        // clean up
        std::fs::remove_dir_all(db_path).unwrap();
    }

    #[test]
    fn can_load_keys() {
        // a hex encoded secret share
        let bashful_secret = KEYGEN_RESULT_INFO_HEX.to_string();
        let bashful_secret_bin = hex::decode(bashful_secret).unwrap();

        assert_ok!(bincode::deserialize::<KeygenResultInfo>(
            bashful_secret_bin.as_ref()
        ));
        let logger = new_test_logger();
        // just a random key
        let key: [u8; 33] = [
            3, 3, 94, 73, 229, 219, 117, 193, 0, 143, 51, 247, 54, 138, 135, 255, 177, 63, 13, 132,
            93, 195, 249, 200, 151, 35, 228, 224, 122, 6, 111, 38, 103,
        ];
        let key_id = KeyId(key.into());
        let db_path = Path::new("db1");
        let _ = std::fs::remove_dir_all(db_path);
        {
            let p_db = PersistentKeyDB::new(&db_path, &logger).unwrap();
            let db = p_db.db;

            let cf = db
                .cf_handle(COLUMN_FAMILY[KEYGEN_DATA_COL])
                .expect("Should handle cf");

            db.put_cf(cf, &key, &bashful_secret_bin)
                .expect("Should write key share");
        }

        {
            let p_db = PersistentKeyDB::new(&db_path, &logger).unwrap();
            let keys = p_db.load_keys();
            let key = keys.get(&key_id).expect("Should have an entry for key");
            assert_eq!(key.params.threshold, 1);
        }
        // clean up
        std::fs::remove_dir_all(db_path).unwrap();
    }

    #[test]
    fn can_update_key() {
        let logger = new_test_logger();
        let key_id = KeyId(vec![0; 33]);
        let db_path = Path::new("db2");
        let _ = std::fs::remove_dir_all(db_path);
        {
            let mut p_db = PersistentKeyDB::new(&db_path, &logger).unwrap();

            let keys_before = p_db.load_keys();
            // there should be no key [0; 33] yet
            assert!(keys_before.get(&key_id).is_none());

            let keygen_result_info = hex::decode(KEYGEN_RESULT_INFO_HEX)
                .expect("Should decode hex to valid KeygenResultInfo binary");
            let keygen_result_info = bincode::deserialize::<KeygenResultInfo>(&keygen_result_info)
                .expect("Should deserialize binary into KeygenResultInfo");
            p_db.update_key(&key_id, &keygen_result_info);

            let keys_before = p_db.load_keys();
            // there should be no key [0; 33] yet
            assert!(keys_before.get(&key_id).is_some());
        }
        // clean up
        std::fs::remove_dir_all(db_path).unwrap();
    }
}
