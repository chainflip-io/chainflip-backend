#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod tests;

use cf_chains::{
	btc::BitcoinCrypto, dot::PolkadotCrypto, evm::EvmCrypto, Bitcoin, Chain, ChainCrypto, Ethereum,
	Polkadot,
};
use cf_primitives::{BroadcastId, CeremonyId, Ed25519PublicKey, EpochIndex, Ipv6Addr, Port};

use codec::{Decode, Encode};
use frame_support::sp_runtime::RuntimeDebug;
use scale_info::TypeInfo;

use sp_std::collections::btree_set::BTreeSet;

#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(C))]
pub struct ThresholdSignatureRequest<ValidatorId, C: ChainCrypto> {
	pub ceremony_id: CeremonyId,
	pub epoch_index: EpochIndex,
	pub key: C::AggKey,
	pub signatories: BTreeSet<ValidatorId>,
	pub payload: C::Payload,
}

#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(C))]
pub struct KeyHandoverRequest<ValidatorId, C: ChainCrypto> {
	pub ceremony_id: CeremonyId,
	pub from_epoch: EpochIndex,
	pub to_epoch: EpochIndex,
	pub key_to_share: C::AggKey,
	pub sharing_participants: BTreeSet<ValidatorId>,
	pub receiving_participants: BTreeSet<ValidatorId>,
	pub new_key: C::AggKey,
}

#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct KeygenRequest<ValidatorId> {
	pub ceremony_id: CeremonyId,
	pub epoch_index: EpochIndex,
	pub participants: BTreeSet<ValidatorId>,
}

#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(T, C))]
pub struct TxBroadcastRequest<ValidatorId, C: Chain> {
	pub broadcast_id: BroadcastId,
	pub nominee: ValidatorId,
	pub payload: C::Transaction,
}

#[derive(Clone, RuntimeDebug, Encode, Decode, PartialEq, Eq, TypeInfo)]
pub enum CfeEvent<ValidatorId> {
	EthThresholdSignatureRequest(ThresholdSignatureRequest<ValidatorId, EvmCrypto>),
	DotThresholdSignatureRequest(ThresholdSignatureRequest<ValidatorId, PolkadotCrypto>),
	BtcThresholdSignatureRequest(ThresholdSignatureRequest<ValidatorId, BitcoinCrypto>),
	EthKeygenRequest(KeygenRequest<ValidatorId>),
	DotKeygenRequest(KeygenRequest<ValidatorId>),
	BtcKeygenRequest(KeygenRequest<ValidatorId>),
	BtcKeyHandoverRequest(KeyHandoverRequest<ValidatorId, BitcoinCrypto>),
	EthTxBroadcastRequest(TxBroadcastRequest<ValidatorId, Ethereum>),
	DotTxBroadcastRequest(TxBroadcastRequest<ValidatorId, Polkadot>),
	BtcTxBroadcastRequest(TxBroadcastRequest<ValidatorId, Bitcoin>),
	PeerIdRegistered { account_id: ValidatorId, pubkey: Ed25519PublicKey, port: Port, ip: Ipv6Addr },
	PeerIdDeregistered { account_id: ValidatorId, pubkey: Ed25519PublicKey },
}
