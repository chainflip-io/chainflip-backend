use std::collections::{HashMap, HashSet};

use pallet_cf_vaults::CeremonyId;

use crate::multisig::{client::KeygenResultInfo, KeyId};

use super::MultisigDB;

#[derive(Clone)]
pub struct MultisigDBMock {
    // Represents a key-value database
    kv_db: HashMap<KeyId, Vec<u8>>,
    used_id_db: Vec<HashSet<CeremonyId>>,
}

impl MultisigDBMock {
    pub fn new() -> Self {
        MultisigDBMock {
            kv_db: HashMap::new(),
            used_id_db: vec![HashSet::new(), HashSet::new()],
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

    fn save_used_ceremony_id(&mut self, ceremony_id: CeremonyId, db_colum: u32) {
        self.used_id_db[(db_colum - 1) as usize].insert(ceremony_id);
    }

    fn remove_used_ceremony_id(&mut self, ceremony_id: &CeremonyId, db_colum: u32) {
        self.used_id_db[(db_colum - 1) as usize].remove(ceremony_id);
    }

    fn load_used_ceremony_ids(&self, db_colum: u32) -> HashSet<CeremonyId> {
        self.used_id_db[(db_colum - 1) as usize].clone()
    }
}
