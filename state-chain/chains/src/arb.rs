//! Types and functions that are common to Arbitrum.
pub mod api;

pub mod benchmarking;

use crate::{
	eth::{DeploymentStatus, EthereumFetchId, SchnorrVerificationComponents},
	*,
};
use cf_primitives::chains::assets;
pub use cf_primitives::chains::Arbitrum;
use codec::{Decode, Encode, MaxEncodedLen};
pub use ethabi::{
	ethereum_types::{H256, U256},
	Address, Hash as TxHash, Token, Uint, Word,
};
use ethereum_types::H160;
use scale_info::TypeInfo;
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_core::ConstBool;
use sp_runtime::RuntimeDebug;
use sp_std::str;

// Reference constants for the chain spec
pub const CHAIN_ID_MAINNET: u64 = 42161;

#[derive(
	Copy,
	Clone,
	RuntimeDebug,
	Default,
	PartialEq,
	Eq,
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	PartialOrd,
	Ord,
)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct ArbitrumAddress(pub [u8; 20]);
impl From<ArbitrumAddress> for H160 {
	fn from(value: ArbitrumAddress) -> Self {
		value.0.into()
	}
}
impl From<H160> for ArbitrumAddress {
	fn from(value: H160) -> Self {
		ArbitrumAddress(*value.as_fixed_bytes())
	}
}
impl From<[u8; 20]> for ArbitrumAddress {
	fn from(value: [u8; 20]) -> Self {
		ArbitrumAddress(value)
	}
}

impl Chain for Arbitrum {
	const NAME: &'static str = "Arbitrum";
	type KeyHandoverIsRequired = ConstBool<false>;
	type OptimisticActivation = ConstBool<false>;
	type ChainBlockNumber = u64;
	type ChainAmount = EthAmount;
	type TransactionFee = eth::TransactionFee;
	type TrackedData = ArbitrumTrackedData;
	type ChainAccount = ArbitrumAddress;
	type ChainAsset = assets::arb::Asset;
	type EpochStartData = ();
	type DepositFetchId = EthereumFetchId;
	type DepositChannelState = DeploymentStatus;
}

impl ChainCrypto for Arbitrum {
	type AggKey = eth::AggKey;
	type Payload = H256;
	type ThresholdSignature = SchnorrVerificationComponents;
	type TransactionInId = H256;
	// We can't use the hash since we don't know it for Ethereum, as we must select an individaul
	// authority to sign the transaction.
	type TransactionOutId = Self::ThresholdSignature;
	type GovKey = Address;

	fn verify_threshold_signature(
		agg_key: &Self::AggKey,
		payload: &Self::Payload,
		signature: &Self::ThresholdSignature,
	) -> bool {
		agg_key
			.verify(payload.as_fixed_bytes(), signature)
			.map_err(|e| log::debug!("Ethereum signature verification failed: {:?}.", e))
			.is_ok()
	}

	fn agg_key_to_payload(agg_key: Self::AggKey) -> Self::Payload {
		H256(Blake2_256::hash(&agg_key.to_pubkey_compressed()))
	}
}

#[derive(
	Copy, Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo,
)]
#[codec(mel_bound())]
pub struct ArbitrumTrackedData {
	pub base_fee: <Arbitrum as Chain>::ChainAmount,
}

impl From<&DepositChannel<Arbitrum>> for EthereumFetchId {
	fn from(channel: &DepositChannel<Arbitrum>) -> Self {
		match channel.state {
			DeploymentStatus::Undeployed => EthereumFetchId::Undeployed(channel.channel_id),
			DeploymentStatus::Pending => EthereumFetchId::Deployed(channel.address.into()),
			DeploymentStatus::Deployed => EthereumFetchId::Deployed(channel.address.into()),
		}
	}
}
