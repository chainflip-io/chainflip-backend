//! Types and functions that are common to Arbitrum.
pub mod api;

pub mod benchmarking;

use crate::{
	evm::{api::EvmChainId, DeploymentStatus, EvmFetchId},
	*,
};
use cf_primitives::chains::assets;
pub use cf_primitives::chains::Arbitrum;
use codec::{Decode, Encode, MaxEncodedLen};
pub use ethabi::{
	ethereum_types::{H256, U256},
	Address, Hash as TxHash, Token, Uint, Word,
};
use frame_support::sp_runtime::RuntimeDebug;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{cmp::min, str};

// Reference constants for the chain spec
pub const CHAIN_ID_MAINNET: u64 = 42161;

impl Chain for Arbitrum {
	const NAME: &'static str = "Arbitrum";
	type ChainBlockNumber = u64;
	type ChainAmount = EthAmount;
	type TransactionFee = evm::TransactionFee;
	type TrackedData = ArbitrumTrackedData;
	type ChainAccount = arb::Address;
	type ChainAsset = assets::arb::Asset;
	type EpochStartData = ();
	type DepositFetchId = EvmFetchId;
	type DepositChannelState = DeploymentStatus;
	type DepositDetails = ();
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

impl From<&DepositChannel<Arbitrum>> for EvmChainId {
	fn from(channel: &DepositChannel<Arbitrum>) -> Self {
		match channel.state {
			DeploymentStatus::Undeployed => EvmChainId::DeployAndFetch(channel.channel_id),
			DeploymentStatus::Pending | DeploymentStatus::Deployed =>
				if channel.asset == assets::arb::Asset::ArbEth {
					EvmChainId::NotRequired
				} else {
					EvmChainId::Fetch(channel.address)
				},
		}
	}
}

impl FeeRefundCalculator<Arbitrum> for evm::Transaction {
	fn return_fee_refund(
		&self,
		fee_paid: <Arbitrum as Chain>::TransactionFee,
	) -> <Arbitrum as Chain>::ChainAmount {
		min(
			self.max_fee_per_gas
				.unwrap_or_default()
				.try_into()
				.expect("In practice `max_fee_per_gas` is always less than u128::MAX"),
			fee_paid.effective_gas_price,
		)
		.saturating_mul(fee_paid.gas_used)
	}
}
