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

//! Types and functions that are common to ethereum.
pub mod api;

pub mod benchmarking;

pub mod deposit_address;

use crate::{
	evm::{DeploymentStatus, EvmFetchId, EvmTransactionMetadata, Transaction},
	Chain, FeeEstimationApi, *,
};
use assets::eth::Asset as EthAsset;
use cf_primitives::chains::assets;
pub use cf_primitives::chains::Ethereum;
use codec::{Decode, Encode, MaxEncodedLen};
pub use ethabi::{ethereum_types::H256, Address, Hash as TxHash, Token, Uint, Word};
use evm::api::EvmReplayProtection;
use frame_support::sp_runtime::{traits::Zero, FixedPointNumber, FixedU64, RuntimeDebug};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_runtime::helpers_128bit::multiply_by_rational_with_rounding;
use sp_std::{cmp::min, convert::TryInto, str};

// Reference constants for the chain spec
pub const CHAIN_ID_MAINNET: u64 = 1;
pub const CHAIN_ID_ROPSTEN: u64 = 3;
#[deprecated]
pub const CHAIN_ID_GOERLI: u64 = 5;
pub const CHAIN_ID_SEPOLIA: u64 = 11155111;
pub const CHAIN_ID_KOVAN: u64 = 42;

pub const REFERENCE_ETH_PRICE_IN_USD: AssetAmount = 2_200_000_000u128; //2200 usd
pub const REFERENCE_FLIP_PRICE_IN_USD: AssetAmount = 330_000u128; //0.33 usd

impl Chain for Ethereum {
	const NAME: &'static str = "Ethereum";
	const GAS_ASSET: Self::ChainAsset = EthAsset::Eth;
	const WITNESS_PERIOD: Self::ChainBlockNumber = 1;

	type ChainCrypto = evm::EvmCrypto;
	type ChainBlockNumber = u64;
	type ChainAmount = EthAmount;
	type TransactionFee = evm::TransactionFee;
	type TrackedData = EthereumTrackedData;
	type ChainAsset = EthAsset;
	type ChainAssetMap<
		T: Member + Parameter + MaxEncodedLen + Copy + BenchmarkValue + FullCodec + Unpin,
	> = assets::eth::AssetMap<T>;
	type ChainAccount = evm::Address;
	type DepositFetchId = EvmFetchId;
	type DepositChannelState = DeploymentStatus;
	type DepositDetails = evm::DepositDetails;
	type Transaction = Transaction;
	type TransactionMetadata = EvmTransactionMetadata;
	type TransactionRef = H256;
	type ReplayProtectionParams = Self::ChainAccount;
	type ReplayProtection = EvmReplayProtection;

	fn input_asset_amount_using_reference_gas_asset_price(
		input_asset: Self::ChainAsset,
		required_gas: Self::ChainAmount,
	) -> Self::ChainAmount {
		match input_asset {
			EthAsset::Usdt | EthAsset::Usdc => multiply_by_rational_with_rounding(
				required_gas,
				REFERENCE_ETH_PRICE_IN_USD,
				1_000_000_000_000_000_000u128,
				sp_runtime::Rounding::Up,
			)
			.unwrap_or(0u128),
			EthAsset::Flip => multiply_by_rational_with_rounding(
				required_gas,
				REFERENCE_ETH_PRICE_IN_USD,
				REFERENCE_FLIP_PRICE_IN_USD,
				sp_runtime::Rounding::Up,
			)
			.unwrap_or(0u128),
			EthAsset::Eth => required_gas,
		}
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
pub struct EthereumTrackedData {
	pub base_fee: <Ethereum as Chain>::ChainAmount,
	pub priority_fee: <Ethereum as Chain>::ChainAmount,
}

impl EthereumTrackedData {
	pub fn max_fee_per_gas(
		&self,
		base_fee_multiplier: FixedU64,
	) -> <Ethereum as Chain>::ChainAmount {
		base_fee_multiplier
			.saturating_mul_int(self.base_fee)
			.saturating_add(self.priority_fee)
	}

	pub fn calculate_ccm_gas_limit(
		&self,
		is_native_asset: bool,
		gas_budget: GasAmount,
		message_length: usize,
	) -> GasAmount {
		use crate::eth::fees::*;
		// Adding one extra gas unit per message's length (byte) for the extra gas overhead of
		// passing the message through the Vault. The extra gas per message's calldata byte
		// should be included in the user's gas budget.
		(gas_budget
			.saturating_add(if is_native_asset {
				CCM_VAULT_NATIVE_GAS_OVERHEAD
			} else {
				CCM_VAULT_TOKEN_GAS_OVERHEAD
			})
			.saturating_add(message_length as u128))
		.min(MAX_GAS_LIMIT)
	}

	pub fn calculate_transaction_fee(
		&self,
		gas_limit: GasAmount,
	) -> <Ethereum as crate::Chain>::ChainAmount {
		(self.base_fee + self.priority_fee).saturating_mul(gas_limit)
	}
}

pub mod fees {
	pub const BASE_COST_PER_BATCH: u128 = 50_000;
	pub const GAS_COST_PER_FETCH: u128 = 30_000;
	pub const GAS_COST_PER_TRANSFER_NATIVE: u128 = 20_000;
	pub const GAS_COST_PER_TRANSFER_TOKEN: u128 = 40_000;
	pub const MAX_GAS_LIMIT: u128 = 10_000_000;
	pub const CCM_VAULT_NATIVE_GAS_OVERHEAD: u128 = 90_000;
	pub const CCM_VAULT_TOKEN_GAS_OVERHEAD: u128 = 120_000;
}

impl FeeEstimationApi<Ethereum> for EthereumTrackedData {
	fn estimate_ingress_fee(
		&self,
		asset: <Ethereum as Chain>::ChainAsset,
	) -> <Ethereum as Chain>::ChainAmount {
		use crate::eth::fees::*;

		// Note: this is taking the egress cost of the swap in the ingress currency (and basing the
		// cost on the ingress chain).
		let gas_cost_per_fetch = BASE_COST_PER_BATCH +
			match asset {
				assets::eth::Asset::Eth => Zero::zero(),
				assets::eth::Asset::Flip | assets::eth::Asset::Usdc | assets::eth::Asset::Usdt =>
					GAS_COST_PER_FETCH,
			};

		self.calculate_transaction_fee(gas_cost_per_fetch)
	}

	fn estimate_ingress_fee_vault_swap(&self) -> Option<<Ethereum as Chain>::ChainAmount> {
		Some(0)
	}

	fn estimate_egress_fee(
		&self,
		asset: <Ethereum as Chain>::ChainAsset,
	) -> <Ethereum as Chain>::ChainAmount {
		use crate::eth::fees::*;

		let gas_cost_per_transfer = BASE_COST_PER_BATCH +
			match asset {
				assets::eth::Asset::Eth => GAS_COST_PER_TRANSFER_NATIVE,
				assets::eth::Asset::Flip | assets::eth::Asset::Usdc | assets::eth::Asset::Usdt =>
					GAS_COST_PER_TRANSFER_TOKEN,
			};

		self.calculate_transaction_fee(gas_cost_per_transfer)
	}

	fn estimate_ccm_fee(
		&self,
		asset: <Ethereum as Chain>::ChainAsset,
		gas_budget: GasAmount,
		message_length: usize,
	) -> Option<<Ethereum as Chain>::ChainAmount> {
		let gas_limit = self.calculate_ccm_gas_limit(
			asset == <Ethereum as Chain>::GAS_ASSET,
			gas_budget,
			message_length,
		);
		Some(self.calculate_transaction_fee(gas_limit))
	}
}

impl Default for EthereumTrackedData {
	#[track_caller]
	fn default() -> Self {
		frame_support::print("You should not use the default chain tracking, as it's meaningless.");

		EthereumTrackedData { base_fee: Default::default(), priority_fee: Default::default() }
	}
}

impl FeeRefundCalculator<Ethereum> for Transaction {
	fn return_fee_refund(
		&self,
		fee_paid: <Ethereum as Chain>::TransactionFee,
	) -> <Ethereum as Chain>::ChainAmount {
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

impl From<&DepositChannel<Ethereum>> for EvmFetchId {
	fn from(channel: &DepositChannel<Ethereum>) -> Self {
		match channel.state {
			DeploymentStatus::Undeployed => EvmFetchId::DeployAndFetch(channel.channel_id),
			DeploymentStatus::Pending | DeploymentStatus::Deployed =>
				if channel.asset == assets::eth::Asset::Eth {
					EvmFetchId::NotRequired
				} else {
					EvmFetchId::Fetch(channel.address)
				},
		}
	}
}

#[cfg(any(test, feature = "runtime-benchmarks"))]
pub mod sig_constants {
	/*
		The below constants have been derived from integration tests with the KeyManager contract.

		In order to check if verification works, we need to use this to construct the AggKey and `SigData` as we
		normally would when submitting a function call to a threshold-signature-protected smart contract.
	*/
	pub const AGG_KEY_PRIV: [u8; 32] =
		hex_literal::hex!("fbcb47bc85b881e0dfb31c872d4e06848f80530ccbd18fc016a27c4a744d0eba");
	pub const AGG_KEY_PUB: [u8; 33] =
		hex_literal::hex!("0331b2ba4b46201610901c5164f42edd1f64ce88076fde2e2c544f9dc3d7b350ae");
	pub const MSG_HASH: [u8; 32] =
		hex_literal::hex!("2bdc19071c7994f088103dbf8d5476d6deb6d55ee005a2f510dc7640055cc84e");
	pub const SIG: [u8; 32] =
		hex_literal::hex!("beb37e87509e15cd88b19fa224441c56acc0e143cb25b9fd1e57fdafed215538");
	pub const SIG_NONCE: [u8; 32] =
		hex_literal::hex!("d51e13c68bf56155a83e50fd9bc840e2a1847fb9b49cd206a577ecd1cd15e285");
}

#[cfg(test)]
mod lifecycle_tests {
	use super::*;
	const ETH: assets::eth::Asset = assets::eth::Asset::Eth;
	const USDC: assets::eth::Asset = assets::eth::Asset::Usdc;

	macro_rules! expect_deposit_state {
		( $state:expr, $asset:expr, $pat:pat ) => {
			cf_utilities::assert_matches!(
				DepositChannel::<Ethereum> {
					channel_id: Default::default(),
					address: Default::default(),
					asset: $asset,
					state: $state,
				}
				.fetch_id(),
				$pat
			);
		};
	}
	#[test]
	fn eth_deposit_address_lifecycle() {
		// Initial state is undeployed.
		let mut state = DeploymentStatus::default();
		assert_eq!(state, DeploymentStatus::Undeployed);
		assert!(state.can_fetch());
		expect_deposit_state!(state, ETH, EvmFetchId::DeployAndFetch(..));
		expect_deposit_state!(state, USDC, EvmFetchId::DeployAndFetch(..));

		// Pending channels can't be fetched from.
		assert!(state.on_fetch_scheduled());
		assert_eq!(state, DeploymentStatus::Pending);
		assert!(!state.can_fetch());

		// Trying to schedule the fetch on a pending channel has no effect.
		assert!(!state.on_fetch_scheduled());
		assert_eq!(state, DeploymentStatus::Pending);
		assert!(!state.can_fetch());

		// On completion, the pending channel is now deployed and be fetched from again.
		assert!(state.on_fetch_completed());
		assert_eq!(state, DeploymentStatus::Deployed);
		assert!(state.can_fetch());
		expect_deposit_state!(state, ETH, EvmFetchId::NotRequired);
		expect_deposit_state!(state, USDC, EvmFetchId::Fetch(..));

		// Channel is now in its final deployed state and be fetched from at any time.
		assert!(!state.on_fetch_scheduled());
		assert!(state.can_fetch());
		assert!(!state.on_fetch_completed());
		assert!(state.can_fetch());
		expect_deposit_state!(state, ETH, EvmFetchId::NotRequired);
		expect_deposit_state!(state, USDC, EvmFetchId::Fetch(..));

		assert_eq!(state, DeploymentStatus::Deployed);
		assert!(!state.on_fetch_scheduled());
	}
}
