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
    fn update_key(&mut self, key_id: &KeyId, keygen_result_info: &KeygenResultInfo) {
        let mut tx = self.db.transaction();

        // TODO: this error should be handled better
        let keygen_result_info_encoded =
            bincode::serialize(keygen_result_info).expect("Could not serialize keygen_result_info");

        tx.put_vec(0, &key_id.0, keygen_result_info_encoded);

        // commit the tx to the database
        self.db.write(tx).expect(&format!(
            "Could not write key share for key_id `{}` to database",
            hex::encode(&key_id.0)
        ));
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

#[cfg(test)]
mod tests {

    use super::*;

    use crate::{
        logging::test_utils::create_test_logger, signing::db::PersistentKeyDB, testing::assert_ok,
    };

    const KEYGEN_RESULT_INFO_HEX: &'static str = "400000000000000062376431363863643863353066376561613263376239356163336464333238396334663238656562316532653530643963313865393534646335613636393464400000000000000062613735626339616534326235363135323337393562373538633362373538373062383338366465393037633637366266633733346535333562633536363034400000000000000064363736343534363131633961323739333431616361396239323364653765316163396339313865646430353434636164623039366437643239323332326364030000000000000040000000000000006334313039333364366365616334386464306562643337333936653230643365626532666566303031353835336462623537313663373533363832316366313740000000000000003964383735353562343139653138643631363035653565333864313639316339313262386432386339373632303330633561653062373034386136366333313640000000000000003463306465383363303731623430346633636433636433623130363462626536313930323230613436393033633932333230303631666662363365356266383703000000000000000100000000000000030000000000000002000000000000004000000000000000363134393934396132616535633561303163323264333461633832306465663834333632653263306334653334356535643066613435616631303632353531334000000000000000326430383662623136663361643130616635623663663864653466303462346632643962346264656464376437373231356161633332383136623337353535664000000000000000336565333634303635333864646433633264363064383961333038643037383436393261323131633232653762316163626164316436383433333130306263614000000000000000646262323030303138326664653233626462383537626361316330633562653466393337613332363839303437613466326339656635313932616231323933630100000000000000030000000000000002000000000000004000000000000000616661386262623133663835383436376437623037376135393035393764333431373231373935333864663532643039323837663062313164623130393334394000000000000000326133373238363963303036636265316139633933376136653531313232633763306566383139393064306239323437623938646237326432663237633839324000000000000000363238383366623530633031343266616539303563343962636263383237356332373064373961646330633130623961643431353261303730393138313935314000000000000000386535663530396632303533613535306432383032626137393534333264383838663335616666343531353732663930393838323433346561366564316236340100000000000000030000000000000002000000000000004000000000000000626137356263396165343262353631353233373935623735386333623735383730623833383664653930376336373662666337333465353335626335363630344000000000000000643637363435343631316339613237393334316163613962393233646537653161633963393138656464303534346361646230393664376432393233323263644000000000000000643365623636356239393238353633613535393730663062346536636533643264323065646164366561636566646638616230616333616130313864623733374000000000000000396361616363393533396633656635633862323333313264303935323039633139363062623530623735383333363839306661343537643763323032376565330300000000000000ca58f2f4ae713dbb3b4db106640a3db150e38007940dfe29e6ebb870c4ccd47e03000000000000008898758bf88855615d459f552e36bfd14e8566c8b368f6a6448942759d5c7f04020000000000000036c0078af3894b8202b541ece6c5d8fb4a091f7e5812b688e7035490404739110100000000000000030000000000000036c0078af3894b8202b541ece6c5d8fb4a091f7e5812b688e7035490404739118898758bf88855615d459f552e36bfd14e8566c8b368f6a6448942759d5c7f04ca58f2f4ae713dbb3b4db106640a3db150e38007940dfe29e6ebb870c4ccd47e01000000000000000300000000000000";

    #[test]
    fn can_load_keys() {
        // a hex encoded secret share
        let bashful_secret = KEYGEN_RESULT_INFO_HEX.to_string();
        let bashful_secret_bin = hex::decode(bashful_secret).unwrap();

        assert_ok!(bincode::deserialize::<KeygenResultInfo>(
            bashful_secret_bin.as_ref()
        ));
        let logger = create_test_logger();
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
