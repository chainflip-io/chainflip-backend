use std::{collections::HashMap, sync::Arc};

use cf_primitives::KeyId;

use crate::multisig::{crypto::CryptoScheme, db::persistent::PersistentKeyDB};

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
		self.keys.get(key_id).or_else(|| {
			// (Temporary) fallback: due to db migration (v0 to v1), some old
			// keys may incorrectly be stored under epoch 0. Check if this is
			// one of those keys and return it if so.
			self.keys
				.get(&KeyId { epoch_index: 0, public_key_bytes: key_id.public_key_bytes.clone() })
		})
	}

	/// Save or update the key data and write it to persistent memory
	pub fn set_key(&mut self, key_id: KeyId, key: KeygenResultInfo<C>) {
		self.db.update_key::<C>(&key_id, &key);
		self.keys.insert(key_id, key);
	}
}

#[test]
fn should_return_key_due_to_fallback() {
	use super::keygen::generate_key_data;
	use crate::multisig::{client::helpers::ACCOUNT_IDS, eth::EthSigning, Rng};
	use rand_legacy::FromEntropy;

	// Create a database just so we can pass it to the key store
	let (_dir, db_file) = utils::testing::new_temp_directory_with_nonexistent_file();
	let db = PersistentKeyDB::open_and_migrate_to_latest(&db_file, None).unwrap();

	let mut key_store = KeyStore::<EthSigning>::new(Arc::new(db));

	// Generate a key and save it under epoch 0 (which is what migration
	// code does for old keys)

	let (key_bytes, key_data) = generate_key_data::<EthSigning>(
		ACCOUNT_IDS.iter().cloned().collect(),
		&mut Rng::from_entropy(),
	);

	let key_id = KeyId { epoch_index: 0, public_key_bytes: key_bytes };

	let key_info = key_data.values().next().unwrap();

	key_store.set_key(key_id.clone(), key_info.clone());

	// Check that we are able to retrieve it using the "correct" epoch
	assert_eq!(key_store.get_key(&KeyId { epoch_index: 10, ..key_id }), Some(key_info));
}
