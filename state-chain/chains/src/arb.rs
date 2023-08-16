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
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_core::ConstBool;
use frame_support::sp_runtime::RuntimeDebug;
use sp_std::str;

// Reference constants for the chain spec
pub const CHAIN_ID_MAINNET: u64 = 42161;

impl Chain for Arbitrum {
	const NAME: &'static str = "Arbitrum";
	type KeyHandoverIsRequired = ConstBool<false>;
	type OptimisticActivation = ConstBool<false>;
	type ChainBlockNumber = u64;
	type ChainAmount = EthAmount;
	type TransactionFee = eth::TransactionFee;
	type TrackedData = ArbitrumTrackedData;
	type ChainAccount = arb::Address;
	type ChainAsset = assets::arb::Asset;
	type EpochStartData = ();
	type DepositFetchId = EthereumFetchId;
	type DepositChannelState = DeploymentStatus;
	type DepositDetails = ();
}

impl ChainCrypto for Arbitrum {
	type AggKey = eth::AggKey;
	type Payload = H256;
	type ThresholdSignature = SchnorrVerificationComponents;
	type TransactionInId = H256;
	// We can't use the hash since we don't know it for Arbitrum, as we must select an individaul
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
			.map_err(|e| log::warn!("Arbitrum signature verification failed: {:?}.", e))
			.is_ok()
	}

	fn agg_key_to_payload(agg_key: Self::AggKey) -> Self::Payload {
		H256(Blake2_256::hash(&agg_key.to_pubkey_compressed()))
	}
}

#[derive(
	Copy,
	Clone,
	RuntimeDebug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	Serialize,
	Deserialize,
)]
#[codec(mel_bound())]
pub struct ArbitrumTrackedData {
	pub base_fee: <Arbitrum as Chain>::ChainAmount,
}

impl Default for ArbitrumTrackedData {
	#[track_caller]
	fn default() -> Self {
		panic!("You should not use the default chain tracking, as it's meaningless.")
	}
}

impl From<&DepositChannel<Arbitrum>> for EthereumFetchId {
	fn from(channel: &DepositChannel<Arbitrum>) -> Self {
		match channel.state {
			DeploymentStatus::Undeployed => EthereumFetchId::DeployAndFetch(channel.channel_id),
			DeploymentStatus::Pending | DeploymentStatus::Deployed =>
				if channel.asset == assets::arb::Asset::ArbEth {
					EthereumFetchId::NotRequired
				} else {
					EthereumFetchId::Fetch(channel.address)
				},
		}
	}
}
