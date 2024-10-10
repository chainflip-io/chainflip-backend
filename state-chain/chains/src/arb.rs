//! Types and functions that are common to Arbitrum.
pub mod api;

pub mod benchmarking;

use crate::{
	evm::{DeploymentStatus, EvmFetchId},
	*,
};
use cf_primitives::chains::assets;
pub use cf_primitives::chains::Arbitrum;
use codec::{Decode, Encode, MaxEncodedLen};
pub use ethabi::{ethereum_types::H256, Address, Hash as TxHash, Token, Uint, Word};
use frame_support::sp_runtime::{traits::Zero, FixedPointNumber, FixedU64, RuntimeDebug};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_core::U256;
use sp_std::{cmp::min, str};

use self::evm::EvmCrypto;

// Reference constants for the chain spec
pub const CHAIN_ID_MAINNET: u64 = 42161;
pub const CHAIN_ID_ARBITRUM_SEPOLIA: u64 = 421614;

impl Chain for Arbitrum {
	const NAME: &'static str = "Arbitrum";
	const GAS_ASSET: Self::ChainAsset = assets::arb::Asset::ArbEth;
	const WITNESS_PERIOD: Self::ChainBlockNumber = 24;

	type ChainCrypto = EvmCrypto;
	type ChainBlockNumber = u64;
	type ChainAmount = EthAmount;
	type TransactionFee = evm::TransactionFee;
	type TrackedData = ArbitrumTrackedData;
	type ChainAsset = assets::arb::Asset;
	type ChainAssetMap<
		T: Member + Parameter + MaxEncodedLen + Copy + BenchmarkValue + FullCodec + Unpin,
	> = assets::arb::AssetMap<T>;
	type ChainAccount = eth::Address;
	type DepositFetchId = EvmFetchId;
	type DepositChannelState = DeploymentStatus;
	type DepositDetails = evm::DepositDetails;
	type Transaction = evm::Transaction;
	type TransactionMetadata = evm::EvmTransactionMetadata;
	type TransactionRef = H256;
	type ReplayProtectionParams = Self::ChainAccount;
	type ReplayProtection = evm::api::EvmReplayProtection;
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
	pub gas_limit_multiplier: FixedU64,
}

impl Default for ArbitrumTrackedData {
	#[track_caller]
	fn default() -> Self {
		frame_support::print("You should not use the default chain tracking, as it's meaningless.");

		ArbitrumTrackedData {
			base_fee: Default::default(),
			gas_limit_multiplier: Default::default(),
		}
	}
}

impl ArbitrumTrackedData {
	pub fn max_fee_per_gas(
		&self,
		base_fee_multiplier: FixedU64,
	) -> <Ethereum as Chain>::ChainAmount {
		base_fee_multiplier.saturating_mul_int(self.base_fee)
	}

	pub fn calculate_ccm_gas_limit(&self, gas_budget: GasAmount) -> U256 {
		use crate::arb::fees::*;

		let gas_limit: U256 = U256::from(gas_budget);

		// TODO: For now we don't differentiate egress native or token. It adds quite some
		// complexity for very little gain.
		// TODO: Do we potentially want to multiply also the gas budget by the multiplier? Then we
		// will be paying for fluctuations but we will simplify the integrator's job as they won't
		// have to worry about it.
		let gas_overhead: u128 = self.gas_limit_multiplier.saturating_mul_int(CCM_GAS_OVERHEAD);
		gas_limit.saturating_add(gas_overhead.into())
	}
}

pub mod fees {
	pub const BASE_COST_PER_BATCH: u128 = 60_000;
	pub const GAS_COST_PER_FETCH: u128 = 30_000;
	pub const GAS_COST_PER_TRANSFER_NATIVE: u128 = 20_000;
	pub const GAS_COST_PER_TRANSFER_TOKEN: u128 = 40_000;
	pub const CCM_GAS_OVERHEAD: u128 = 123; // TODO: To estimate
}

impl FeeEstimationApi<Arbitrum> for ArbitrumTrackedData {
	fn estimate_ingress_fee(
		&self,
		asset: <Arbitrum as Chain>::ChainAsset,
	) -> <Arbitrum as Chain>::ChainAmount {
		use crate::arb::fees::*;

		// Note: this is taking the egress cost of the swap in the ingress currency (and basing the
		// cost on the ingress chain).
		let gas_cost_per_fetch = BASE_COST_PER_BATCH +
			match asset {
				assets::arb::Asset::ArbEth => Zero::zero(),
				assets::arb::Asset::ArbUsdc => GAS_COST_PER_FETCH,
			};

		self.base_fee
			.saturating_mul(self.gas_limit_multiplier.saturating_mul_int(gas_cost_per_fetch))
	}

	fn estimate_egress_fee(
		&self,
		asset: <Arbitrum as Chain>::ChainAsset,
	) -> <Arbitrum as Chain>::ChainAmount {
		use crate::arb::fees::*;

		let gas_cost_per_transfer = BASE_COST_PER_BATCH +
			match asset {
				assets::arb::Asset::ArbEth => GAS_COST_PER_TRANSFER_NATIVE,
				assets::arb::Asset::ArbUsdc => GAS_COST_PER_TRANSFER_TOKEN,
			};

		self.base_fee
			.saturating_mul(self.gas_limit_multiplier.saturating_mul_int(gas_cost_per_transfer))
	}
}

impl From<&DepositChannel<Arbitrum>> for EvmFetchId {
	fn from(channel: &DepositChannel<Arbitrum>) -> Self {
		match channel.state {
			DeploymentStatus::Undeployed => EvmFetchId::DeployAndFetch(channel.channel_id),
			DeploymentStatus::Pending | DeploymentStatus::Deployed =>
				if channel.asset == assets::arb::Asset::ArbEth {
					EvmFetchId::NotRequired
				} else {
					EvmFetchId::Fetch(channel.address)
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
