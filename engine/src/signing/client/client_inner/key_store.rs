use std::collections::HashMap;

use crate::signing::{db::KeyDB, KeyId};

use super::common::KeygenResultInfo;

// Successfully generated multisig keys live here
#[derive(Clone)]
pub struct KeyStore<S>
where
    S: KeyDB,
{
    keys: HashMap<KeyId, KeygenResultInfo>,
    db: S,
}

// TODO: this will need to be fixed to get the keys via the ceremony id or the pubkey
// Not sure if the above TODO is actually true now
impl<S> KeyStore<S>
where
    S: KeyDB,
{
    pub fn new(db: S) -> Self {
        let keys = db.load_keys();

        KeyStore { keys, db }
    }

    #[cfg(test)]
    pub fn get_db(&self) -> &S {
        &self.db
    }

    pub fn get_key(&self, key_id: KeyId) -> Option<&KeygenResultInfo> {
        self.keys.get(&key_id)
    }

    // Save `key` under key `key_id` overwriting if exists
    pub fn set_key(&mut self, key_id: KeyId, key: KeygenResultInfo) {
        self.db.update_key(key_id.clone(), &key);
        self.keys.insert(key_id, key);
    }
}
