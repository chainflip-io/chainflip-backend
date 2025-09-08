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

use cf_chains::mocks::{MockEthereum, MockEthereumChainCrypto};
use codec::{Decode, Encode};
use frame_support::{storage, StorageHasher, Twox64Concat};
use sp_std::vec::Vec;

use crate::{CfeBroadcastRequest, CfeMultisigRequest, CfePeerRegistration, Chainflip};

pub struct MockCfeInterface {}

#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq)]
pub enum MockCfeEvent<ValidatorId> {
	EvmThresholdSignatureRequest(
		cfe_events::ThresholdSignatureRequest<ValidatorId, MockEthereumChainCrypto>,
	),
	EthTxBroadcastRequest(cfe_events::TxBroadcastRequest<ValidatorId, MockEthereum>),
	EvmKeygenRequest(cfe_events::KeygenRequest<ValidatorId>),
	// Note: we don't normally do handover for eth, but this works for tests
	EthKeyHandoverRequest(cfe_events::KeyHandoverRequest<ValidatorId, MockEthereumChainCrypto>),
	PeerIdRegistered {
		account_id: ValidatorId,
		pubkey: cf_primitives::Ed25519PublicKey,
		port: u16,
		ip: cf_primitives::Ipv6Addr,
	},
	PeerIdDeregistered {
		account_id: ValidatorId,
		pubkey: cf_primitives::Ed25519PublicKey,
	},
}

const STORAGE_KEY: &[u8] = b"MockCfeInterface::Events";

impl<T: Chainflip> CfeMultisigRequest<T, MockEthereumChainCrypto> for MockCfeInterface {
	fn keygen_request(req: cfe_events::KeygenRequest<T::ValidatorId>) {
		Self::append_event(MockCfeEvent::EvmKeygenRequest(req));
	}

	fn signature_request(
		req: cfe_events::ThresholdSignatureRequest<T::ValidatorId, MockEthereumChainCrypto>,
	) {
		Self::append_event(MockCfeEvent::EvmThresholdSignatureRequest(req));
	}

	fn key_handover_request(
		req: cfe_events::KeyHandoverRequest<<T as Chainflip>::ValidatorId, MockEthereumChainCrypto>,
	) {
		Self::append_event(MockCfeEvent::EthKeyHandoverRequest(req));
	}
}

impl<T: Chainflip> CfeBroadcastRequest<T, MockEthereum> for MockCfeInterface {
	fn tx_broadcast_request(req: cfe_events::TxBroadcastRequest<T::ValidatorId, MockEthereum>) {
		Self::append_event(MockCfeEvent::EthTxBroadcastRequest(req));
	}
}

impl<T: Chainflip> CfePeerRegistration<T> for MockCfeInterface {
	fn peer_registered(
		account_id: <T as Chainflip>::ValidatorId,
		pubkey: cf_primitives::Ed25519PublicKey,
		port: u16,
		ip: cf_primitives::Ipv6Addr,
	) {
		Self::append_event(MockCfeEvent::PeerIdRegistered { account_id, pubkey, port, ip });
	}

	fn peer_deregistered(
		account_id: <T as Chainflip>::ValidatorId,
		pubkey: cf_primitives::Ed25519PublicKey,
	) {
		Self::append_event(MockCfeEvent::PeerIdDeregistered { account_id, pubkey });
	}
}

impl MockCfeInterface {
	pub fn take_events<ValidatorId>() -> Vec<MockCfeEvent<ValidatorId>>
	where
		MockCfeEvent<ValidatorId>: Decode,
	{
		storage::hashed::take_or_default(&<Twox64Concat as StorageHasher>::hash, STORAGE_KEY)
	}

	fn store_events<ValidatorId: Encode>(events: Vec<MockCfeEvent<ValidatorId>>) {
		storage::hashed::put(&<Twox64Concat as StorageHasher>::hash, STORAGE_KEY, &events);
	}

	fn append_event<ValidatorId: Encode>(event: MockCfeEvent<ValidatorId>)
	where
		MockCfeEvent<ValidatorId>: Decode,
	{
		let mut existing_events = Self::take_events::<ValidatorId>();
		existing_events.push(event);
		Self::store_events(existing_events);
	}
}
