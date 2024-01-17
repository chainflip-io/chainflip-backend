#![cfg(debug_assertions)]

use cf_chains::mocks::{MockEthereum, MockEthereumChainCrypto};
use codec::{Decode, Encode};
use frame_support::{storage, StorageHasher, Twox64Concat};

use crate::{CfeBroadcastRequest, CfeMultisigRequest, CfePeerRegistration, Chainflip};

pub struct MockCfeInterface {}

#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq)]
pub enum MockCfeEvent<ValidatorId> {
	EthThresholdSignatureRequest(
		cfe_events::ThresholdSignatureRequest<ValidatorId, MockEthereumChainCrypto>,
	),
	EthTxBroadcastRequest(cfe_events::TxBroadcastRequest<ValidatorId, MockEthereum>),
	EthKeygenRequest(cfe_events::KeygenRequest<ValidatorId>),
	// Note: we don't normally do handover for eth, but this works for tests
	EthKeyHandoverRequest(cfe_events::KeyHandoverRequest<ValidatorId, MockEthereumChainCrypto>),
}

const STORAGE_KEY: &[u8] = b"MockCfeInterface::Events";

impl<T: Chainflip> CfeMultisigRequest<T, MockEthereumChainCrypto> for MockCfeInterface {
	fn keygen_request(req: cfe_events::KeygenRequest<T::ValidatorId>) {
		Self::append_event(MockCfeEvent::EthKeygenRequest(req));
	}

	fn signature_request(
		req: cfe_events::ThresholdSignatureRequest<T::ValidatorId, MockEthereumChainCrypto>,
	) {
		Self::append_event(MockCfeEvent::EthThresholdSignatureRequest(req));
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
		_account_id: <T as Chainflip>::ValidatorId,
		_pubkey: cf_primitives::Ed25519PublicKey,
		_port: u16,
		_ip: cf_primitives::Ipv6Addr,
	) {
		// TODO: implement when needed for any test
	}

	fn peer_deregistered(
		_account_id: <T as Chainflip>::ValidatorId,
		_pubkey: cf_primitives::Ed25519PublicKey,
	) {
		// TODO: implement when needed for any test
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
