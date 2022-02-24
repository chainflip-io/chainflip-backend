use std::collections::{HashMap, HashSet};

use pallet_cf_vaults::CeremonyId;

use crate::multisig::{client::KeygenResultInfo, KeyId};

use super::KeyDB;

#[derive(Clone, Default)]
pub struct KeyDBMock {
    // Represents a key-value database
    kv_db: HashMap<KeyId, Vec<u8>>,
    signing_tracking_data: HashSet<CeremonyId>,
    keygen_tracking_data: HashSet<CeremonyId>,
}

impl KeyDB for KeyDBMock {
    fn update_key(&mut self, key_id: &KeyId, key: &KeygenResultInfo) {
        let val = bincode::serialize(key).expect("Should be serializable key");

        self.kv_db.insert(key_id.to_owned(), val);
    }

    fn load_keys(&self) -> HashMap<KeyId, KeygenResultInfo> {
        self.kv_db
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    bincode::deserialize::<KeygenResultInfo>(v)
                        .expect("Invalid data for KeygenResultInfo"),
                )
            })
            .collect()
    }
}
