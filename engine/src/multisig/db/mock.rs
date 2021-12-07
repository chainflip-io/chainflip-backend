use std::collections::{HashMap, HashSet};

use pallet_cf_vaults::CeremonyId;

use crate::multisig::{client::KeygenResultInfo, KeyId};

use super::KeyDB;

#[derive(Clone)]
pub struct KeyDBMock {
    // Represents a key-value database
    kv_db: HashMap<KeyId, Vec<u8>>,
    used_id_window: Option<(CeremonyId, CeremonyId)>,
    unused_id_db: HashSet<CeremonyId>,
}

impl KeyDBMock {
    pub fn new() -> Self {
        KeyDBMock {
            kv_db: HashMap::new(),
            used_id_window: None,
            unused_id_db: HashSet::new(),
        }
    }
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

    fn update_used_ceremony_id_window(&mut self, window: (CeremonyId, CeremonyId)) {
        self.used_id_window = Some(window);
    }

    fn save_unused_ceremony_id(&mut self, ceremony_id: CeremonyId) {
        self.unused_id_db.insert(ceremony_id);
    }

    fn remove_unused_ceremony_id(&mut self, ceremony_id: &CeremonyId) {
        self.unused_id_db.remove(ceremony_id);
    }

    fn load_unused_ceremony_ids(&self) -> HashSet<CeremonyId> {
        self.unused_id_db.clone()
    }

    fn load_used_ceremony_id_window(&self) -> Option<(CeremonyId, CeremonyId)> {
        self.used_id_window
    }
}
