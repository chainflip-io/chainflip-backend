use std::{collections::BTreeSet, sync::Arc};

use super::*;
use crate::{
	multisig::{
		client::{
			self,
			common::SigningFailureReason,
			helpers::{
				new_nodes, ACCOUNT_IDS, DEFAULT_KEYGEN_CEREMONY_ID, DEFAULT_SIGNING_CEREMONY_ID,
			},
			key_store::KeyStore,
			CeremonyRequestDetails,
		},
		eth::EthSigning,
		PersistentKeyDB,
	},
	testing::{assert_future_can_complete, new_temp_directory_with_nonexistent_file},
};

use cf_primitives::{KeyId, GENESIS_EPOCH};
use client::MultisigClient;
use utilities::{assert_err, assert_ok};

#[tokio::test]
async fn should_ignore_rts_for_unknown_key() {
	let account_id = &ACCOUNT_IDS[0];
	let (_dir, db_file) = new_temp_directory_with_nonexistent_file();

	// Use any key id, as the key db will be empty
	let key_id = KeyId { epoch_index: GENESIS_EPOCH, public_key_bytes: Vec::from([0u8; 32]) };

	// Create a client
	let (ceremony_request_sender, _) = tokio::sync::mpsc::unbounded_channel();
	let client = MultisigClient::<EthSigning>::new(
		account_id.clone(),
		KeyStore::new(Arc::new(
			PersistentKeyDB::open_and_migrate_to_latest(&db_file, None)
				.expect("Failed to open database"),
		)),
		ceremony_request_sender,
	);

	// Send Sign Request
	let signing_request_fut = client.initiate_signing(
		DEFAULT_SIGNING_CEREMONY_ID,
		key_id,
		BTreeSet::from_iter(ACCOUNT_IDS.iter().cloned()),
		vec![EthSigning::signing_payload_for_test()],
	);

	// Check sign request fails immediately with "unknown key" error
	let (_, failure_reason) = assert_err!(assert_future_can_complete(signing_request_fut));
	assert_eq!(failure_reason, SigningFailureReason::UnknownKey);
}

#[tokio::test]
async fn should_save_key_after_keygen() {
	let (_dir, db_file) = new_temp_directory_with_nonexistent_file();

	// Generate a key to use in this test
	let (public_key_bytes, keygen_result_info) = {
		let (public_key_bytes, key_data) =
			helpers::run_keygen(new_nodes(ACCOUNT_IDS.clone()), DEFAULT_KEYGEN_CEREMONY_ID).await;
		(public_key_bytes, key_data.into_iter().next().unwrap().1)
	};

	{
		// Create a client
		let (ceremony_request_sender, mut ceremony_request_receiver) =
			tokio::sync::mpsc::unbounded_channel();
		let client = MultisigClient::<EthSigning>::new(
			ACCOUNT_IDS[0].clone(),
			KeyStore::new(Arc::new(
				PersistentKeyDB::open_and_migrate_to_latest(&db_file, None)
					.expect("Failed to open database"),
			)),
			ceremony_request_sender,
		);

		// Send Keygen Request
		let keygen_request_fut = client.initiate_keygen(
			DEFAULT_KEYGEN_CEREMONY_ID,
			GENESIS_EPOCH,
			BTreeSet::from_iter(ACCOUNT_IDS.iter().cloned()),
		);

		// Get the oneshot channel that is linked to the keygen request
		// and send a successful keygen result
		let request = ceremony_request_receiver.recv().await.unwrap();
		match request.details.unwrap() {
			CeremonyRequestDetails::Keygen(details) => {
				details.result_sender.send(Ok(keygen_result_info)).unwrap();
			},
			_ => {
				panic!("Unexpected ceremony request");
			},
		}

		// Complete the keygen request
		assert_ok!(keygen_request_fut.await);
	}

	// Check that the key was saved by Loading it from the same db file
	let key_store = KeyStore::<EthSigning>::new(Arc::new(
		PersistentKeyDB::open_and_migrate_to_latest(&db_file, None)
			.expect("Failed to open database"),
	));
	assert!(
		key_store
			.get_key(&KeyId { epoch_index: GENESIS_EPOCH, public_key_bytes })
			.is_some(),
		"Key not found in db"
	);
}

#[tokio::test]
async fn should_load_keys_on_creation() {
	// Generate a key to use in this test
	let (public_key_bytes, stored_keygen_result_info) = {
		let (public_key_bytes, key_data) =
			helpers::run_keygen(new_nodes(ACCOUNT_IDS.clone()), DEFAULT_KEYGEN_CEREMONY_ID).await;
		(public_key_bytes, key_data.into_iter().next().unwrap().1)
	};

	// A temp directory to store the key db for this test
	let (_dir, db_file) = new_temp_directory_with_nonexistent_file();

	let key_id = KeyId { epoch_index: GENESIS_EPOCH, public_key_bytes };

	// Create a new db and store the key in it
	{
		let mut key_store = KeyStore::<EthSigning>::new(Arc::new(
			PersistentKeyDB::open_and_migrate_to_latest(&db_file, None)
				.expect("Failed to open database"),
		));
		key_store.set_key(key_id.clone(), stored_keygen_result_info.clone());
	}

	// Create the client using the existing db file
	let (ceremony_request_sender, _) = tokio::sync::mpsc::unbounded_channel();
	let client = MultisigClient::<EthSigning>::new(
		ACCOUNT_IDS[0].clone(),
		KeyStore::new(Arc::new(
			PersistentKeyDB::open_and_migrate_to_latest(&db_file, None)
				.expect("Failed to open database"),
		)),
		ceremony_request_sender,
	);

	// Check that the key was loaded during the creation of the client and it matches the original
	// key
	assert_eq!(
		*client.key_store.lock().unwrap().get_key(&key_id).expect("Key not found in db"),
		stored_keygen_result_info
	);
}
