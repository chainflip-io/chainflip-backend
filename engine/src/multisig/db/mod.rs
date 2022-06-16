pub mod persistent;

use std::collections::HashMap;

pub use persistent::PersistentKeyDB;

use super::{client::KeygenResultInfo, crypto::ECPoint, KeyId};

// Successfully generated multisig keys live here
pub struct KeyStore<P>
where
    P: ECPoint,
{
    keys: HashMap<KeyId, KeygenResultInfo<P>>,
    db: PersistentKeyDB<P>,
}

impl<P> KeyStore<P>
where
    P: ECPoint,
{
    /// Load the keys from persistent memory and put them into a new keystore
    pub fn new(db: PersistentKeyDB<P>) -> Self {
        let keys = db.load_keys();

        KeyStore { keys, db }
    }

    /// Get the key for the given key id
    pub fn get_key(&self, key_id: &KeyId) -> Option<&KeygenResultInfo<P>> {
        self.keys.get(key_id)
    }

    /// Save or update the key data and write it to persistent memory
    pub fn set_key(&mut self, key_id: KeyId, key: KeygenResultInfo<P>) {
        self.db.update_key(&key_id, &key);
        self.keys.insert(key_id, key);
    }
}
