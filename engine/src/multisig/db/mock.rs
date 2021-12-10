use std::collections::{HashMap, HashSet};

use pallet_cf_vaults::CeremonyId;

use crate::multisig::{client::KeygenResultInfo, KeyId};

use super::MultisigDB;

#[derive(Clone)]
pub struct MultisigDBMock {
    // Represents a key-value database
    kv_db: HashMap<KeyId, Vec<u8>>,
    signing_tracking_data: HashSet<CeremonyId>,
    keygen_tracking_data: HashSet<CeremonyId>,
}

impl MultisigDBMock {
    pub fn new() -> Self {
        MultisigDBMock {
            kv_db: HashMap::new(),
            signing_tracking_data: HashSet::new(),
            keygen_tracking_data: HashSet::new(),
        }
    }
}

impl MultisigDB for MultisigDBMock {
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

    /// Save a new unused ceremony id to the underlying storage
    fn update_tracking_for_signing(&mut self, data: &HashSet<CeremonyId>) {
        self.signing_tracking_data = data.clone();
    }
    fn update_tracking_for_keygen(&mut self, data: &HashSet<CeremonyId>) {
        self.keygen_tracking_data = data.clone();
    }

    /// Load all the unused ceremony ids from the underlying storage
    fn load_tracking_for_signing(&self) -> HashSet<CeremonyId> {
        self.signing_tracking_data.clone()
    }
    fn load_tracking_for_keygen(&self) -> HashSet<CeremonyId> {
        self.keygen_tracking_data.clone()
    }
}
