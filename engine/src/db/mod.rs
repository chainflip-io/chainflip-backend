pub mod persistent;
use std::{collections::HashMap, sync::Arc};

pub use persistent::PersistentKeyDB;

use multisig::{
	client::{key_store_api::KeyStoreAPI, KeygenResultInfo},
	CryptoScheme, KeyId,
};

/// A gateway for accessing key data from persistent memory
pub struct KeyStore<C>
where
	C: CryptoScheme,
{
	keys: HashMap<KeyId, KeygenResultInfo<C>>,
	db: Arc<PersistentKeyDB>,
}

impl<C: CryptoScheme> KeyStore<C> {
	/// Load the keys from persistent memory and put them into a new keystore
	pub fn new(db: Arc<PersistentKeyDB>) -> Self {
		KeyStore { keys: db.load_keys::<C>(), db }
	}
}

impl<C: CryptoScheme> KeyStoreAPI<C> for KeyStore<C> {
	fn get_key(&self, key_id: &KeyId) -> Option<KeygenResultInfo<C>> {
		self.keys.get(key_id).cloned()
	}

	fn set_key(&mut self, key_id: KeyId, key: KeygenResultInfo<C>) {
		self.db.update_key::<C>(&key_id, &key);
		self.keys.insert(key_id, key);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::db::PersistentKeyDB;
	use cf_primitives::AccountId;
	use multisig::{client::keygen, eth::EthSigning, CanonicalEncoding, Rng};
	use rand_legacy::FromEntropy;
	use std::collections::BTreeSet;

	// The `new` function of the keystore should load all keys from the db.
	// This test also covers that the `set_key` function saves the key to the db and not just the
	// hashmap.
	#[tokio::test]
	async fn should_load_keys_on_creation() {
		// Generate a key to use in this test
		let (public_key, key_data) = keygen::generate_key_data::<EthSigning>(
			BTreeSet::from([AccountId::new([1; 32])]),
			&mut Rng::from_entropy(),
		);

		let stored_keygen_result_info = key_data.values().next().unwrap().clone();

		// A temp directory to store the key db for this test
		let (_dir, db_file) = utilities::testing::new_temp_directory_with_nonexistent_file();

		let key_id = KeyId { epoch_index: 0, public_key_bytes: public_key.encode_key() };

		// Create a new db and use the keystore to save the key
		{
			let mut key_store = KeyStore::<EthSigning>::new(Arc::new(
				PersistentKeyDB::open_and_migrate_to_latest(&db_file, None)
					.expect("Failed to open database"),
			));
			assert!(key_store.keys.is_empty(), "The db should be empty");
			key_store.set_key(key_id.clone(), stored_keygen_result_info.clone());
		}

		// Create the keystore again
		let key_store = KeyStore::<EthSigning>::new(Arc::new(
			PersistentKeyDB::open_and_migrate_to_latest(&db_file, None)
				.expect("Failed to open database"),
		));

		// Check that the key was loaded during the creation of the keystore
		assert_eq!(
			key_store.get_key(&key_id).expect("Key not found in db"),
			stored_keygen_result_info
		);
	}
}
