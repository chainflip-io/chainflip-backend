use std::{collections::HashMap, sync::Arc};

use crate::multisig::{crypto::CryptoScheme, db::persistent::PersistentKeyDB, KeyId};

use super::KeygenResultInfo;

// Successfully generated multisig keys live here
pub struct KeyStore<C>
where
	C: CryptoScheme,
{
	keys: HashMap<KeyId, KeygenResultInfo<C>>,
	db: Arc<PersistentKeyDB>,
}

impl<C> KeyStore<C>
where
	C: CryptoScheme,
{
	/// Load the keys from persistent memory and put them into a new keystore
	pub fn new(db: Arc<PersistentKeyDB>) -> Self {
		let keys = db.load_keys::<C>();

		KeyStore { keys, db }
	}

	/// Get the key for the given key id
	pub fn get_key(&self, key_id: &KeyId) -> Option<&KeygenResultInfo<C>> {
		self.keys.get(key_id)
	}

	/// Save or update the key data and write it to persistent memory
	pub fn set_key(&mut self, key_id: KeyId, key: KeygenResultInfo<C>) {
		self.db.update_key::<C>(&key_id, &key);
		self.keys.insert(key_id, key);
	}
}
