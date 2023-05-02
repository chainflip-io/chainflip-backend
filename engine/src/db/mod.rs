pub mod persistent;
use std::{collections::HashMap, sync::Arc};

use cf_chains::KeyId;
pub use persistent::PersistentKeyDB;

use crate::multisig::{
	client::{key_store_api::KeyStoreAPI, KeygenResultInfo},
	CryptoScheme,
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
		self.keys
			.get(key_id)
			.or_else(|| {
				// (Temporary) fallback: due to db migration (v0 to v1), some old
				// keys may incorrectly be stored under epoch 0. Check if this is
				// one of those keys and return it if so.
				self.keys.get(&KeyId {
					epoch_index: 0,
					public_key_bytes: key_id.public_key_bytes.clone(),
				})
			})
			.cloned()
	}

	fn set_key(&mut self, key_id: KeyId, key: KeygenResultInfo<C>) {
		self.db.update_key::<C>(&key_id, &key);
		self.keys.insert(key_id, key);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::multisig::{
		client::{keygen, keygen::generate_key_data},
		eth::EthSigning,
		PersistentKeyDB, Rng,
	};
	use cf_primitives::AccountId;
	use rand_legacy::FromEntropy;
	use std::collections::BTreeSet;

	#[test]
	fn should_return_key_due_to_fallback() {
		let account_ids: BTreeSet<_> = [1, 2, 3].iter().map(|i| AccountId::new([*i; 32])).collect();

		// Create a database just so we can pass it to the key store
		let (_dir, db_file) = utilities::testing::new_temp_directory_with_nonexistent_file();
		let db = PersistentKeyDB::open_and_migrate_to_latest(&db_file, None).unwrap();

		let mut key_store = KeyStore::<EthSigning>::new(Arc::new(db));

		// Generate a key and save it under epoch 0 (which is what migration
		// code does for old keys)

		let (key_bytes, key_data) = generate_key_data::<EthSigning>(
			account_ids.iter().cloned().collect(),
			&mut Rng::from_entropy(),
		);

		let key_id = KeyId { epoch_index: 0, public_key_bytes: key_bytes };

		let key_info = key_data.values().next().unwrap().clone();

		key_store.set_key(key_id.clone(), key_info.clone());

		// Check that we are able to retrieve it using the "correct" epoch
		assert_eq!(key_store.get_key(&KeyId { epoch_index: 10, ..key_id }), Some(key_info));
	}

	// The `new` function of the keystore should load all keys from the db.
	// This test also covers that the `set_key` function saves the key to the db and not just the
	// hashmap.
	#[tokio::test]
	async fn should_load_keys_on_creation() {
		// Generate a key to use in this test
		let (public_key_bytes, key_data) = keygen::generate_key_data::<EthSigning>(
			BTreeSet::from([AccountId::new([1; 32])]),
			&mut Rng::from_entropy(),
		);

		let stored_keygen_result_info = key_data.values().next().unwrap().clone();

		// A temp directory to store the key db for this test
		let (_dir, db_file) = utilities::testing::new_temp_directory_with_nonexistent_file();

		let key_id = KeyId { epoch_index: 0, public_key_bytes };

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
