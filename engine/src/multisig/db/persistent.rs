use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use super::MultisigDB;
use kvdb_rocksdb::{Database, DatabaseConfig};
use pallet_cf_vaults::CeremonyId;
use slog::o;

use crate::{
    logging::COMPONENT_KEY,
    multisig::{client::KeygenResultInfo, KeyId},
};

pub const DB_COL_KEYGEN_RESULT_INFO: u32 = 0;
//pub const DB_COL_CEREMONY_TRACKING: u32 = 1;

pub const DB_KEY_SIGNING_TRACKING_DATA: &[u8] = b"signing_tracking_data";
pub const DB_KEY_KEYGEN_TRACKING_DATA: &[u8] = b"keygen_tracking_data";

/// Database for keys that uses rocksdb
pub struct PersistentMultisigDB {
    /// Rocksdb database instance
    db: Database,
    logger: slog::Logger,
}

impl PersistentMultisigDB {
    pub fn new(path: &Path, logger: &slog::Logger) -> Self {
        let config = DatabaseConfig::default();
        // TODO: Update to kvdb 14 and then can pass in &Path
        let db = Database::open(&config, path.to_str().expect("Invalid path"))
            .expect("could not open database");

        PersistentMultisigDB {
            db,
            logger: logger.new(o!(COMPONENT_KEY => "PersistentMultisigDB")),
        }
    }
}

impl MultisigDB for PersistentMultisigDB {
    fn update_key(&mut self, key_id: &KeyId, keygen_result_info: &KeygenResultInfo) {
        let mut tx = self.db.transaction();

        // TODO: this error should be handled better
        let keygen_result_info_encoded =
            bincode::serialize(keygen_result_info).expect("Could not serialize keygen_result_info");

        tx.put_vec(
            DB_COL_KEYGEN_RESULT_INFO,
            &key_id.0,
            keygen_result_info_encoded,
        );

        // commit the tx to the database
        self.db.write(tx).unwrap_or_else(|e| {
            panic!(
                "Could not write key share for key_id `{}` to database: {}",
                &key_id, e,
            )
        });
    }

    fn load_keys(&self) -> HashMap<KeyId, KeygenResultInfo> {
        self.db
            .iter(DB_COL_KEYGEN_RESULT_INFO)
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

    fn update_tracking_for_signing(&mut self, data: &HashSet<CeremonyId>) {
        save_ceremony_tracking(&mut self.db, data, DB_KEY_SIGNING_TRACKING_DATA);
    }

    fn load_tracking_for_signing(&self) -> HashSet<CeremonyId> {
        load_ceremony_tracking(&self.db, DB_KEY_SIGNING_TRACKING_DATA)
            .expect("should load signing tacking data")
    }

    fn update_tracking_for_keygen(&mut self, data: &HashSet<CeremonyId>) {
        save_ceremony_tracking(&mut self.db, data, DB_KEY_KEYGEN_TRACKING_DATA);
    }

    fn load_tracking_for_keygen(&self) -> HashSet<CeremonyId> {
        load_ceremony_tracking(&self.db, DB_KEY_KEYGEN_TRACKING_DATA)
            .expect("should load keygen tacking data")
    }
}

fn save_ceremony_tracking(_db: &mut Database, _data: &HashSet<CeremonyId>, _key: &[u8]) {
    // Disabled writing ceremony tracking data to disk until we have a better db solution.
    /*
    let mut tx = db.transaction();

    let data_encoded = bincode::serialize(data).expect("Could not serialize hashset");

    tx.put_vec(DB_COL_CEREMONY_TRACKING, key, data_encoded);

    // Commit the tx to the database
    db.write(tx)
        .unwrap_or_else(|e| panic!("Could not write hashset `{:?}` to database: {}", data, e,));
    */
}

fn load_ceremony_tracking(_db: &Database, _key: &[u8]) -> anyhow::Result<HashSet<CeremonyId>> {
    // Disabled writing ceremony tracking data to disk until we have a better db solution.
    /*
    match db
        .get(DB_COL_CEREMONY_TRACKING, key)
        .expect("should load ceremony tracking hashset")
    {
        Some(data) => bincode::deserialize::<HashSet<CeremonyId>>(&data)
            .map_err(anyhow::Error::new)
            .with_context(|| "Could not deserialize ceremony tracking data"),
        None => Ok(HashSet::new()),
    }
    */
    Ok(HashSet::new())
}

#[cfg(test)]
mod tests {

    use std::iter::FromIterator;

    use super::*;

    use crate::{
        logging::test_utils::new_test_logger, multisig::db::PersistentMultisigDB,
        testing::assert_ok,
    };

    // To generate this, you can use the test in engine/src/signing/client/client_inner/genesis.rs
    const KEYGEN_RESULT_INFO_HEX: &'static str = "2100000000000000024f836dd72086a52a5861d83616cd1b589f9b4928e5797786136be857e97117de20000000000000000aed87281bb3d1bfbf4bae120a2f2a5185a8e03904151eb5d8ea216d3c7900a90300000000000000210000000000000002fc79eec21733244e32a942398023d32f34df1caf2c4b301fdcdc7f7ed4fc22272100000000000000032e1b86241836d2ad7e92d231f02419fc154066e86c7c48951fc8aeae9f55d6872100000000000000022063ee40a562f461f71f75b3d7675383bb66fdb9f15078b2d8db76f063b56bc103000000000000008898758bf88855615d459f552e36bfd14e8566c8b368f6a6448942759d5c7f040200000000000000ca58f2f4ae713dbb3b4db106640a3db150e38007940dfe29e6ebb870c4ccd47e030000000000000036c0078af3894b8202b541ece6c5d8fb4a091f7e5812b688e7035490404739110100000000000000030000000000000036c0078af3894b8202b541ece6c5d8fb4a091f7e5812b688e7035490404739118898758bf88855615d459f552e36bfd14e8566c8b368f6a6448942759d5c7f04ca58f2f4ae713dbb3b4db106640a3db150e38007940dfe29e6ebb870c4ccd47e03000000000000000100000000000000";

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
            let p_db = PersistentMultisigDB::new(&db_path, &logger);
            let db = p_db.db;

            // Add the keyshare to the database
            let mut tx = db.transaction();
            tx.put_vec(0, &key, bashful_secret_bin);
            db.write(tx).unwrap();
        }

        let p_db = PersistentMultisigDB::new(&db_path, &logger);
        let keys = p_db.load_keys();
        let key = keys.get(&key_id).expect("Should have an entry for key");
        assert_eq!(key.params.threshold, 1);
        // clean up
        std::fs::remove_dir_all(db_path).unwrap();
    }

    #[ignore = "Disabled writing ceremony tracking data to disk until we have a better db solution"]
    #[test]
    fn can_update_key() {
        let logger = new_test_logger();
        let key_id = KeyId(vec![0; 33]);
        let db_path = Path::new("db2");
        let _ = std::fs::remove_dir_all(db_path);
        {
            let mut p_db = PersistentMultisigDB::new(&db_path, &logger);

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

    #[ignore = "Disabled writing ceremony tracking data to disk until we have a better db solution"]
    #[test]
    fn can_save_and_load_used_ceremony_id_data() {
        let logger = new_test_logger();
        let db_path = Path::new("db3");
        let _ = std::fs::remove_dir_all(db_path);

        let mut p_db = PersistentMultisigDB::new(&db_path, &logger);

        // Save some ids
        let mut signing_hashset: HashSet<CeremonyId> =
            HashSet::from_iter(vec![1, 2].iter().cloned());
        p_db.update_tracking_for_signing(&signing_hashset);
        assert_eq!(p_db.load_tracking_for_signing(), signing_hashset);

        // Remove an id and load again
        signing_hashset.remove(&signing_hashset.iter().last().unwrap().clone());
        p_db.update_tracking_for_signing(&signing_hashset);
        assert_eq!(p_db.load_tracking_for_signing(), signing_hashset);

        // Save some ids
        let mut keygen_hashset: HashSet<CeremonyId> =
            HashSet::from_iter(vec![3, 4].iter().cloned());
        p_db.update_tracking_for_keygen(&keygen_hashset);
        assert_eq!(p_db.load_tracking_for_keygen(), keygen_hashset);

        // Remove an id and load again
        keygen_hashset.remove(&keygen_hashset.iter().last().unwrap().clone());
        p_db.update_tracking_for_keygen(&keygen_hashset);
        assert_eq!(p_db.load_tracking_for_keygen(), keygen_hashset);

        // Cleanup
        std::fs::remove_dir_all(db_path).unwrap();
    }
}
