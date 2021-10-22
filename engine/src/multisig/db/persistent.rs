use std::{collections::HashMap, convert::TryInto, path::Path};

use super::KeyDB;
use kvdb_rocksdb::{Database, DatabaseConfig};
use slog::o;

use crate::{
    logging::COMPONENT_KEY,
    multisig::{client::KeygenResultInfo, KeyId},
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
    fn update_key(&mut self, key_id: &KeyId, keygen_result_info: &KeygenResultInfo) {
        let mut tx = self.db.transaction();

        // TODO: this error should be handled better
        let keygen_result_info_encoded =
            bincode::serialize(keygen_result_info).expect("Could not serialize keygen_result_info");

        tx.put_vec(0, &key_id.0, keygen_result_info_encoded);

        // commit the tx to the database
        self.db.write(tx).unwrap_or_else(|e| {
            panic!(
                "Could not write key share for key_id `{}` to database: {}",
                hex::encode(&key_id.0),
                e,
            )
        });
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
                    Ok(keygen_info) => Some((key_id, keygen_info)),
                    Err(err) => {
                        slog::error!(
                            self.logger,
                            "Could not deserialize key_info (key_id: {:?}) from DB: {}",
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
        logging::test_utils::create_test_logger, multisig::db::PersistentKeyDB, testing::assert_ok,
    };

    // To generate this, you can use the test in engine/src/signing/client/client_inner/genesis.rs
    const KEYGEN_RESULT_INFO_HEX: &'static str = "40000000000000003339653330326634356530353934396662623334376530633662626132323464383264323237613730313634303135386263316337393930393137343730313540000000000000006265626534346235303432616432373630386662326465306233383636303330366130303530613432626439366162396161633937333839636165383131613140000000000000006163303662313238303235353564383562623032373564303336316332313139316534356236623337326432626161663532396238313465643035343436346603000000000000003f00000000000000336532363261373164393534623561396131366562383038313735376336373438303230383739616437666431373266653938653635356436653235653131400000000000000037633335353736333037626539376136333866626430353635643533623935383032386531343632343932366339643365313465306662633239316530313037400000000000000036386136633631326238353661303766333638383065323433616539616235633330303761393537383639336437616663326439333732383565346430383966400000000000000032333037343630386635396163393362353636343037653234646139303333623637306163616434323334313732323666663438313136396262623966363739400000000000000039366562326563303261646233313233636164633731316464333432656164323230653261313562663865653762363239326330396562353062356265323534400000000000000033633535353133663635396438303862656339313939653031643531313530656431623038626535343734383564383138653133633637323233633938353934030000000000000036c0078af3894b8202b541ece6c5d8fb4a091f7e5812b688e7035490404739110100000000000000ca58f2f4ae713dbb3b4db106640a3db150e38007940dfe29e6ebb870c4ccd47e03000000000000008898758bf88855615d459f552e36bfd14e8566c8b368f6a6448942759d5c7f040200000000000000030000000000000036c0078af3894b8202b541ece6c5d8fb4a091f7e5812b688e7035490404739118898758bf88855615d459f552e36bfd14e8566c8b368f6a6448942759d5c7f04ca58f2f4ae713dbb3b4db106640a3db150e38007940dfe29e6ebb870c4ccd47e01000000000000000300000000000000";

    #[test]
    fn can_load_keys() {
        // a hex encoded secret share
        let bashful_secret = KEYGEN_RESULT_INFO_HEX.to_string();
        let bashful_secret_bin = hex::decode(bashful_secret).unwrap();

        assert_ok!(bincode::deserialize::<KeygenResultInfo>(
            bashful_secret_bin.as_ref()
        ));
        let logger = create_test_logger();
        // just a random key
        let key: [u8; 33] = [
            3, 3, 94, 73, 229, 219, 117, 193, 0, 143, 51, 247, 54, 138, 135, 255, 177, 63, 13, 132,
            93, 195, 249, 200, 151, 35, 228, 224, 122, 6, 111, 38, 103,
        ];
        let key_id = KeyId(key.into());
        let db_path = Path::new("db1");
        {
            // Insert the key into the database
            let p_db = PersistentKeyDB::new(&db_path, &logger);
            let db = p_db.db;

            // Add the keyshare to the database
            let mut tx = db.transaction();
            tx.put_vec(0, &key, bashful_secret_bin);
            db.write(tx).unwrap();
        }

        let p_db = PersistentKeyDB::new(&db_path, &logger);
        let keys = p_db.load_keys();
        keys.get(&key_id).expect("Should have an entry for key");
        // clean up
        std::fs::remove_dir_all(db_path).unwrap();
    }

    #[test]
    fn can_update_key() {
        let logger = create_test_logger();
        let key_id = KeyId(vec![0; 33]);
        let db_path = Path::new("db2");
        {
            let mut p_db = PersistentKeyDB::new(&db_path, &logger);

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
