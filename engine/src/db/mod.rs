pub mod persistent;
use std::{collections::HashMap, sync::Arc};

use cf_primitives::KeyId;
pub use persistent::PersistentKeyDB;

use crate::multisig::{
	client::{key_store::KeyStoreAPI, KeygenResultInfo},
	CryptoScheme,
};

// Successfully generated multisig keys live here
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
		let keys = db.load_keys::<C>();

		KeyStore { keys, db }
	}
}

impl<C: CryptoScheme> KeyStoreAPI<C> for KeyStore<C> {
	// TODO JAMIE: getting a lifetime problem with &KeygenResultInfo here. take another look later.
	// or maybe just leave it, seems it gets cloned anyway during signing.
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

#[test]
fn should_return_key_due_to_fallback() {
	use crate::multisig::{client::keygen::generate_key_data, eth::EthSigning, Rng};
	use cf_primitives::AccountId;
	use rand_legacy::FromEntropy;
	use std::collections::BTreeSet;

	let account_ids: BTreeSet<_> = [1, 2, 3].iter().map(|i| AccountId::new([*i; 32])).collect();

	// Create a database just so we can pass it to the key store
	let (_dir, db_file) = utils::testing::new_temp_directory_with_nonexistent_file();
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

// TODO JAMIE: fix this test
// #[tokio::test]
// async fn should_load_keys_on_creation() {
// 	// Generate a key to use in this test
// 	let (public_key_bytes, stored_keygen_result_info) = {
// 		let (public_key_bytes, key_data) =
// 			helpers::run_keygen(new_nodes(ACCOUNT_IDS.clone()), DEFAULT_KEYGEN_CEREMONY_ID).await;
// 		(public_key_bytes, key_data.into_iter().next().unwrap().1)
// 	};

// 	// A temp directory to store the key db for this test
// 	let (_dir, db_file) = utils::testing::new_temp_directory_with_nonexistent_file();

// 	let key_id = KeyId { epoch_index: GENESIS_EPOCH, public_key_bytes };

// 	// Create a new db and store the key in it
// 	{
// 		let mut key_store = KeyStore::<EthSigning>::new(Arc::new(
// 			PersistentKeyDB::open_and_migrate_to_latest(&db_file, None)
// 				.expect("Failed to open database"),
// 		));
// 		key_store.set_key(key_id.clone(), stored_keygen_result_info.clone());
// 	}

// 	// Create the client using the existing db file
// 	let (ceremony_request_sender, _) = tokio::sync::mpsc::unbounded_channel();
// 	let client = MultisigClient::<EthSigning>::new(
// 		ACCOUNT_IDS[0].clone(),
// 		KeyStore::new(Arc::new(
// 			PersistentKeyDB::open_and_migrate_to_latest(&db_file, None)
// 				.expect("Failed to open database"),
// 		)),
// 		ceremony_request_sender,
// 	);

// 	// Check that the key was loaded during the creation of the client and it matches the original
// 	// key
// 	assert_eq!(
// 		*client.key_store.lock().unwrap().get_key(&key_id).expect("Key not found in db"),
// 		stored_keygen_result_info
// 	);
// }
