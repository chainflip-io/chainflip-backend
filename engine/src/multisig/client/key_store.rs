use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::multisig::{crypto::CryptoScheme, db::persistent::PersistentKeyDB, KeyId};

use super::KeygenResultInfo;

// Successfully generated multisig keys live here
pub struct KeyStore<C>
where
    C: CryptoScheme,
{
    keys: HashMap<KeyId, KeygenResultInfo<C::Point>>,
    db: Arc<Mutex<PersistentKeyDB<C>>>,
}

impl<C> KeyStore<C>
where
    C: CryptoScheme,
{
    /// Load the keys from persistent memory and put them into a new keystore
    pub fn new(db: Arc<Mutex<PersistentKeyDB<C>>>) -> Self {
        let keys = db.lock().expect("should get lock").load_keys();

        KeyStore { keys, db }
    }

    /// Get the key for the given key id
    pub fn get_key(&self, key_id: &KeyId) -> Option<&KeygenResultInfo<C::Point>> {
        self.keys.get(key_id)
    }

    /// Save or update the key data and write it to persistent memory
    pub fn set_key(&mut self, key_id: KeyId, key: KeygenResultInfo<C::Point>) {
        self.db
            .lock()
            .expect("should get lock")
            .update_key(&key_id, &key);
        self.keys.insert(key_id, key);
    }
}
