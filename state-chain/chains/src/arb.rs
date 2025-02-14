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
	pub l1_base_fee_estimate: <Arbitrum as Chain>::ChainAmount,
}

impl Default for ArbitrumTrackedData {
	#[track_caller]
	fn default() -> Self {
		frame_support::print("You should not use the default chain tracking, as it's meaningless.");

		ArbitrumTrackedData {
			base_fee: Default::default(),
			l1_base_fee_estimate: Default::default(),
		}
	}
}

impl ArbitrumTrackedData {
	pub fn max_fee_per_gas(
		&self,
		base_fee_multiplier: FixedU64,
	) -> <Arbitrum as Chain>::ChainAmount {
		base_fee_multiplier.saturating_mul_int(self.base_fee)
	}

	// Estimating gas as described in Arbitrum's docs:
	// https://docs.arbitrum.io/build-decentralized-apps/how-to-estimate-gas
	//
	// We assume the user's gas budget is just the amount of gas they need on the receiver
	// smart contract. Chainflip computes the entire overhead of the Vault transaction and
	// adjusts the final gas limit according to the L1 fees, including the user's part of
	// the transaction(gas budget).
	pub fn calculate_ccm_gas_limit(
		&self,
		is_native_asset: bool,
		gas_budget: GasAmount,
		message_length: usize,
	) -> GasAmount {
		use crate::arb::fees::*;

		let vault_gas_overhead = if is_native_asset {
			CCM_VAULT_NATIVE_GAS_OVERHEAD
		} else {
			CCM_VAULT_TOKEN_GAS_OVERHEAD
		};

		// Adding one extra gas unit per message's length (byte) for the extra gas overhead of
		// passing the message through the Vault. The extra l2 gas per message's calldata byte
		// should be included in the user's gas budget together with the receiving logic's gas
		// required.
		let l2g = vault_gas_overhead.saturating_add(message_length as u128).saturating_add(gas_budget);
		let l1p = self.l1_base_fee_estimate * L1_GAS_PER_BYTES;
		let p = self.base_fee;

		let l1s = CCM_VAULT_BYTES_OVERHEAD + CCM_BUFFER_BYTES_OVERHEAD + CCM_ARBITRUM_BYTES_OVERHEAD + message_length as u128;

		let l1c = l1p.saturating_mul(l1s);

		let b = l1c.div_ceil(p);

		let gas_limit = l2g.saturating_add(b);
		gas_limit.saturating_add(gas_budget).min(MAX_GAS_LIMIT)
	}

	pub fn calculate_transaction_fee(
		&self,
		gas_limit: GasAmount,
	) -> <Arbitrum as crate::Chain>::ChainAmount {
		self.base_fee.saturating_mul(gas_limit)
	}
}

pub mod fees {
	pub const BASE_COST_PER_BATCH: u128 = 60_000;
	pub const GAS_COST_PER_FETCH: u128 = 30_000;
	pub const GAS_COST_PER_TRANSFER_NATIVE: u128 = 20_000;
	pub const GAS_COST_PER_TRANSFER_TOKEN: u128 = 40_000;
	pub const MAX_GAS_LIMIT: u128 = 25_000_000;
	pub const CCM_VAULT_NATIVE_GAS_OVERHEAD: u128 = 90_000;
	pub const CCM_VAULT_TOKEN_GAS_OVERHEAD: u128 = 120_000;
	// Arbitrum specific ccm gas limit calculation constants
	pub const CCM_VAULT_BYTES_OVERHEAD: u128 = 356;
	pub const CCM_ARBITRUM_BYTES_OVERHEAD: u128 = 140;
	// This is an extra buffer added to ensure that the user will
	// receive the desired gas amount. Might need to be adjusted
	// according to Arbitrum's compression rate.
	pub const CCM_BUFFER_BYTES_OVERHEAD: u128 = 100; // ~33%
	pub const L1_GAS_PER_BYTES: u128 = 16;
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

		self.calculate_transaction_fee(gas_cost_per_fetch)
	}

	fn estimate_ingress_fee_vault_swap(&self) -> Option<<Arbitrum as Chain>::ChainAmount> {
		Some(0)
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

		self.calculate_transaction_fee(gas_cost_per_transfer)
	}

	fn estimate_ccm_fee(
		&self,
		asset: <Arbitrum as Chain>::ChainAsset,
		gas_budget: GasAmount,
		message_length: usize,
	) -> Option<<Arbitrum as Chain>::ChainAmount> {
		let gas_limit = self.calculate_ccm_gas_limit(
			asset == <Arbitrum as Chain>::GAS_ASSET,
			gas_budget,
			message_length,
		);
		Some(self.calculate_transaction_fee(gas_limit))
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

#[cfg(test)]
mod test {
	use super::*;
	use crate::arb::fees::*;

	#[test]
	fn calculate_gas_limit() {
		const GAS_BUDGET: u128 = 80_000u128;
		const MESSAGE_LENGTH: usize = 1;
		const L1_BASE_FEE_ESTIMATE: u128 = 26_920_712_879u128;

		let arb_tracked_data = ArbitrumTrackedData {
			base_fee: 100_000_000u128,
			l1_base_fee_estimate: L1_BASE_FEE_ESTIMATE,
		};

		let gas_limit = arb_tracked_data.calculate_ccm_gas_limit(true, GAS_BUDGET, MESSAGE_LENGTH);
		assert_eq!(gas_limit, 2526102u128);

		let gas_budget_extra = 1_000_000u128;
		let gas_limit_extra = arb_tracked_data.calculate_ccm_gas_limit(
			true,
			GAS_BUDGET + gas_budget_extra,
			MESSAGE_LENGTH,
		);
		assert_eq!(gas_limit + gas_budget_extra, gas_limit_extra);

		let gas_limit_token =
			arb_tracked_data.calculate_ccm_gas_limit(false, GAS_BUDGET, MESSAGE_LENGTH);
		assert_eq!(
			gas_limit_token,
			gas_limit + CCM_VAULT_TOKEN_GAS_OVERHEAD - CCM_VAULT_NATIVE_GAS_OVERHEAD
		);
		let gas_limit_token_extra = arb_tracked_data.calculate_ccm_gas_limit(
			false,
			GAS_BUDGET + gas_budget_extra,
			MESSAGE_LENGTH,
		);
		assert_eq!(gas_limit_token + gas_budget_extra, gas_limit_token_extra);
	}

	#[test]
	fn gas_limit_cap() {
		const GAS_BUDGET: u128 = 80_000u128;

		let arb_tracked_data = ArbitrumTrackedData {
			base_fee: 100_000_000u128,
			l1_base_fee_estimate: 26_920_712_879u128,
		};

		for is_native_asset in [true, false].iter() {
			let mut gas_limit =
				arb_tracked_data.calculate_ccm_gas_limit(*is_native_asset, GAS_BUDGET, 1);
			let gas_limit_diff = MAX_GAS_LIMIT - gas_limit;
			gas_limit = arb_tracked_data.calculate_ccm_gas_limit(
				*is_native_asset,
				GAS_BUDGET + gas_limit_diff,
				1,
			);
			assert_eq!(gas_limit, MAX_GAS_LIMIT);

			gas_limit = arb_tracked_data.calculate_ccm_gas_limit(
				*is_native_asset,
				GAS_BUDGET + gas_limit_diff + 1,
				1,
			);
			assert_eq!(gas_limit, MAX_GAS_LIMIT);
		}
	}
}
