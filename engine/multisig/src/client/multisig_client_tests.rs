// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

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
	eth::{EthSigning, EvmCryptoScheme},
};
use mockall::predicate;

use crate::client::key_store_api::MockKeyStoreAPI;
use cf_primitives::GENESIS_EPOCH;
use cf_utilities::{assert_err, assert_matches, assert_ok, testing::assert_future_can_complete};
use client::MultisigClient;

#[tokio::test]
async fn should_ignore_rts_for_unknown_key() {
	let account_id = &ACCOUNT_IDS[0];

	// Make the keystore return `None` when asked for a key
	let mut mock_key_store = MockKeyStoreAPI::new();
	mock_key_store.expect_get_key().once().returning(|_| None);

	let (ceremony_request_sender, mut ceremony_request_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	// Create a client
	let client = MultisigClient::<EthSigning, _>::new(
		account_id.clone(),
		mock_key_store,
		ceremony_request_sender,
	);

	// Send a signing request
	let signing_request_fut = client.initiate_signing(
		DEFAULT_SIGNING_CEREMONY_ID,
		BTreeSet::from_iter(ACCOUNT_IDS.iter().cloned()),
		vec![(KeyId::new(GENESIS_EPOCH, [0u8; 32]), EvmCryptoScheme::signing_payload_for_test())],
	);

	// Check that the signing request fails immediately with an "unknown key" error
	let (_, failure_reason) = assert_err!(assert_future_can_complete(signing_request_fut));
	assert_eq!(failure_reason, SigningFailureReason::MissingKey);
	assert_matches!(
		assert_ok!(assert_future_can_complete(ceremony_request_receiver.recv())),
		CeremonyRequest { ceremony_id: DEFAULT_SIGNING_CEREMONY_ID, details: None }
	);
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
			predicate::eq(KeyId::new(GENESIS_EPOCH, public_key)),
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
