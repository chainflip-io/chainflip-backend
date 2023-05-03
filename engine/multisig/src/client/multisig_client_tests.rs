use std::collections::BTreeSet;

use super::*;
use crate::{
	client::{
		self,
		common::SigningFailureReason,
		helpers::{
			new_nodes, ACCOUNT_IDS, DEFAULT_KEYGEN_CEREMONY_ID, DEFAULT_SIGNING_CEREMONY_ID,
		},
		CeremonyRequestDetails, KeyId,
	},
	eth::EthSigning,
};
use mockall::predicate;

use crate::client::key_store_api::MockKeyStoreAPI;
use cf_primitives::GENESIS_EPOCH;
use client::MultisigClient;
use utilities::{assert_err, assert_ok, testing::assert_future_can_complete};

#[tokio::test]
async fn should_ignore_rts_for_unknown_key() {
	let account_id = &ACCOUNT_IDS[0];

	// Make the keystore return `None` when asked for a key
	let mut mock_key_store = MockKeyStoreAPI::new();
	mock_key_store.expect_get_key().once().returning(|_| None);

	// Create a client
	let client = MultisigClient::<EthSigning, _>::new(
		account_id.clone(),
		mock_key_store,
		tokio::sync::mpsc::unbounded_channel().0,
	);

	// Send a signing request
	let signing_request_fut = client.initiate_signing(
		DEFAULT_SIGNING_CEREMONY_ID,
		KeyId { epoch_index: GENESIS_EPOCH, public_key_bytes: Vec::from([0u8; 32]) },
		BTreeSet::from_iter(ACCOUNT_IDS.iter().cloned()),
		vec![EthSigning::signing_payload_for_test()],
	);

	// Check that the signing request fails immediately with an "unknown key" error
	let (_, failure_reason) = assert_err!(assert_future_can_complete(signing_request_fut));
	assert_eq!(failure_reason, SigningFailureReason::UnknownKey);
}

#[tokio::test]
async fn should_save_key_after_keygen() {
	// Generate a key to use in this test
	let (public_key, keygen_result_info) = {
		let (public_key, key_data) =
			helpers::run_keygen(new_nodes(ACCOUNT_IDS.clone()), DEFAULT_KEYGEN_CEREMONY_ID).await;
		(public_key, key_data.into_iter().next().unwrap().1)
	};

	// Make sure that the `set_key` function is called once with correct key data
	let mut mock_key_store = MockKeyStoreAPI::<EthSigning>::new();
	mock_key_store
		.expect_set_key()
		.with(
			predicate::eq(KeyId {
				epoch_index: GENESIS_EPOCH,
				public_key_bytes: public_key.encode_key(),
			}),
			predicate::eq(keygen_result_info.clone()),
		)
		.once()
		.returning(|_, _| ());

	// Create a client
	let (ceremony_request_sender, mut ceremony_request_receiver) =
		tokio::sync::mpsc::unbounded_channel();
	let client = MultisigClient::<EthSigning, _>::new(
		ACCOUNT_IDS[0].clone(),
		mock_key_store,
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
