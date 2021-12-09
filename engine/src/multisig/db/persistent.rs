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

/// Database for keys that uses rocksdb
pub struct PersistentMultisigDB {
    /// Rocksdb database instance
    db: Database,
    logger: slog::Logger,
}

impl PersistentMultisigDB {
    pub fn new(path: &Path, logger: &slog::Logger) -> Self {
        let config = DatabaseConfig::with_columns(2);
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

    fn save_used_ceremony_id(&mut self, ceremony_id: CeremonyId, db_colum: u32) {
        assert_ne!(
            db_colum, DB_COL_KEYGEN_RESULT_INFO,
            "Db colum {} is reserved for keys",
            db_colum
        );

        let mut tx = self.db.transaction();

        let ceremony_id_encoded =
            bincode::serialize(&ceremony_id).expect("Could not serialize ceremony_id");

        tx.put_vec(db_colum, &ceremony_id_encoded.clone(), ceremony_id_encoded);

        // Commit the tx to the database
        self.db.write(tx).unwrap_or_else(|e| {
            panic!(
                "Could not write ceremony_id `{:?}` to database: {}",
                ceremony_id, e,
            )
        });
    }

    fn remove_used_ceremony_id(&mut self, ceremony_id: &CeremonyId, db_colum: u32) {
        assert_ne!(
            db_colum, DB_COL_KEYGEN_RESULT_INFO,
            "Db colum {} is reserved for keys",
            db_colum
        );

        let mut tx = self.db.transaction();
        let ceremony_id_encoded =
            bincode::serialize(&ceremony_id).expect("Could not serialize ceremony_id");
        tx.delete(db_colum, &ceremony_id_encoded);

        // Commit the tx to the database
        self.db.write(tx).unwrap_or_else(|e| {
            panic!(
                "Could not delete ceremony_id `{:?}` from database: {}",
                ceremony_id, e,
            )
        });
    }

    fn load_used_ceremony_ids(&self, db_colum: u32) -> HashSet<CeremonyId> {
        assert_ne!(
            db_colum, DB_COL_KEYGEN_RESULT_INFO,
            "Db colum {} is reserved for keys",
            db_colum
        );

        self.db
            .iter(db_colum)
            .filter_map(
                |(_, data)| match bincode::deserialize::<CeremonyId>(&data) {
                    Ok(ceremony_id) => Some(ceremony_id),
                    Err(_) => None,
                },
            )
            .collect()
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    use crate::{
        logging::test_utils::new_test_logger, multisig::db::PersistentMultisigDB,
        testing::assert_ok,
    };

    // To generate this, you can use the test in engine/src/signing/client/client_inner/genesis.rs
    const KEYGEN_RESULT_INFO_HEX: &'static str = "4000000000000000653631616664363737636466626563383338633666333039646566663062326336303536663861323766326337383362363862626136623330663636376265364000000000000000393735623536383538393264643062356564623633626133386638313137643538356631633636353836643030396362313365646162383232633663353764634000000000000000323932666137623232666364636564316533313830343331373835653935343134633361373833623436663265616537303236386634353033623839633634650300000000000000400000000000000061323132343466626463646362623431303963383861383437663032393632333033626437353066393166353238376165663034366263356364346363656531400000000000000034623063363464366131343130653363613965383266643566623264653966353436313761396466373563633363353935336137666561633462613136626664400000000000000031626162336533313632353133333738613736383466623762663537383563363339316464306662313830303361383063336161323631363561343738643461400000000000000037623035633931343736633363316164336365396166623164656364633331393434663737643632333438373837386463623230383734616161346633626339400000000000000039616439376432383734633362373233656238316338326239386665663836353830393962396632333533313466353735643263386232326138643563343636400000000000000031653662653966363466353631383232653535646433653637356531316433623636666333363332666436343462316236633335626430336333643063343637030000000000000036c0078af3894b8202b541ece6c5d8fb4a091f7e5812b688e70354904047391101000000000000008898758bf88855615d459f552e36bfd14e8566c8b368f6a6448942759d5c7f040200000000000000ca58f2f4ae713dbb3b4db106640a3db150e38007940dfe29e6ebb870c4ccd47e0300000000000000030000000000000036c0078af3894b8202b541ece6c5d8fb4a091f7e5812b688e7035490404739118898758bf88855615d459f552e36bfd14e8566c8b368f6a6448942759d5c7f04ca58f2f4ae713dbb3b4db106640a3db150e38007940dfe29e6ebb870c4ccd47e03000000000000000100000000000000";

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

    #[test]
    fn can_update_key() {
        let logger = new_test_logger();
        let key_id = KeyId(vec![0; 33]);
        let db_path = Path::new("db2");
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

    #[test]
    fn can_save_and_load_used_ceremony_id_data() {
        let logger = new_test_logger();
        let db_path = Path::new("db3");
        let db_colum = 1;

        let mut p_db = PersistentMultisigDB::new(&db_path, &logger);

        // Save and load the id hash set
        let mut test_used_ids: HashSet<CeremonyId> = HashSet::new();
        test_used_ids.insert(42);
        test_used_ids.insert(50);

        p_db.save_used_ceremony_id(42, db_colum);
        p_db.save_used_ceremony_id(50, db_colum);

        let loaded_unused_ids = p_db.load_used_ceremony_ids(db_colum);

        assert_eq!(loaded_unused_ids, test_used_ids);

        // Remove an entry
        p_db.remove_used_ceremony_id(&50, db_colum);
        test_used_ids.remove(&50);
        let loaded_unused_ids = p_db.load_used_ceremony_ids(db_colum);

        assert_eq!(loaded_unused_ids, test_used_ids);

        std::fs::remove_dir_all(db_path).unwrap();
    }
}
