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

//! Types and functions that are common to BSC (Binance Smart Chain).
pub mod api;

pub mod benchmarking;

use crate::{
	evm::{DeploymentStatus, EvmFetchId},
	ChainWitnessConfig, *,
};
use cf_primitives::chains::assets;
pub use cf_primitives::chains::Bsc;
use codec::{Decode, Encode, MaxEncodedLen};
pub use ethabi::{
	ethereum_types::{H160, H256},
	Address, Hash as TxHash, Token, Uint, Word,
};
use frame_support::sp_runtime::{traits::Zero, FixedPointNumber, FixedU64, RuntimeDebug};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{cmp::min, str};

use self::evm::EvmCrypto;

// Reference constants for the chain spec
pub const CHAIN_ID_MAINNET: u64 = 56;
pub const CHAIN_ID_TESTNET: u64 = 97;

impl ChainWitnessConfig for Bsc {
	type ChainBlockNumber = u64;
	const WITNESS_PERIOD: Self::ChainBlockNumber = 24;
}

impl Chain for Bsc {
	const NAME: &'static str = "Bsc";
	const GAS_ASSET: Self::ChainAsset = assets::bsc::Asset::BscBnb;
	const WITNESS_PERIOD: Self::ChainBlockNumber = 24;
	const FINE_AMOUNT_PER_UNIT: Self::ChainAmount = eth::ONE_ETH;
	const BURN_ADDRESS: Self::ChainAccount = H160([0; 20]);

	type ChainCrypto = EvmCrypto;
	type ChainBlockNumber = u64;
	type ChainAmount = EthAmount;
	type TransactionFee = evm::TransactionFee;
	type TrackedData = BscTrackedData;
	type ChainAsset = assets::bsc::Asset;
	type ChainAssetMap<
		T: Member + Parameter + MaxEncodedLen + Copy + BenchmarkValue + FullCodec + Unpin,
	> = assets::bsc::AssetMap<T>;
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
	PartialOrd,
	Ord,
)]
#[codec(mel_bound())]
pub struct BscTrackedData {
	pub base_fee: <Bsc as Chain>::ChainAmount,
}

impl Default for BscTrackedData {
	#[track_caller]
	fn default() -> Self {
		frame_support::print("You should not use the default chain tracking, as it's meaningless.");

		BscTrackedData { base_fee: Default::default() }
	}
}

impl BscTrackedData {
	pub fn max_fee_per_gas(&self, base_fee_multiplier: FixedU64) -> <Bsc as Chain>::ChainAmount {
		base_fee_multiplier.saturating_mul_int(self.base_fee)
	}

	pub fn calculate_ccm_gas_limit(
		&self,
		is_native_asset: bool,
		gas_budget: GasAmount,
		message_length: usize,
	) -> GasAmount {
		use crate::bsc::fees::*;

		let vault_gas_overhead = if is_native_asset {
			CCM_VAULT_NATIVE_GAS_OVERHEAD
		} else {
			CCM_VAULT_TOKEN_GAS_OVERHEAD
		};

		// Add gas for vault overhead plus message length
		let gas_limit = vault_gas_overhead.saturating_add(message_length as u128);
		gas_limit.saturating_add(gas_budget).min(MAX_GAS_LIMIT)
	}

	pub fn calculate_transaction_fee(
		&self,
		gas_limit: GasAmount,
	) -> <Bsc as crate::Chain>::ChainAmount {
		self.base_fee.saturating_mul(gas_limit)
	}
}

// todo: revisit these constants
pub mod fees {
	pub const BASE_COST_PER_BATCH: u128 = 50_000;
	pub const GAS_COST_PER_FETCH: u128 = 30_000;
	pub const GAS_COST_PER_TRANSFER_NATIVE: u128 = 20_000;
	pub const GAS_COST_PER_TRANSFER_TOKEN: u128 = 40_000;
	pub const MAX_GAS_LIMIT: u128 = 10_000_000;
	pub const CCM_VAULT_NATIVE_GAS_OVERHEAD: u128 = 90_000;
	pub const CCM_VAULT_TOKEN_GAS_OVERHEAD: u128 = 120_000;
}

impl FeeEstimationApi<Bsc> for BscTrackedData {
	fn estimate_fee(
		&self,
		asset: <Bsc as Chain>::ChainAsset,
		ingress_or_egress: IngressOrEgress,
	) -> <Bsc as Chain>::ChainAmount {
		use crate::bsc::fees::*;

		match ingress_or_egress {
			IngressOrEgress::IngressDepositChannel => {
				let gas_cost_per_fetch = BASE_COST_PER_BATCH +
					match asset {
						assets::bsc::Asset::BscBnb => Zero::zero(),
						assets::bsc::Asset::BscUsdt => GAS_COST_PER_FETCH,
					};

				self.calculate_transaction_fee(gas_cost_per_fetch)
			},
			IngressOrEgress::IngressVaultSwap => 0,
			IngressOrEgress::Egress => {
				let gas_cost_per_transfer = BASE_COST_PER_BATCH +
					match asset {
						assets::bsc::Asset::BscBnb => GAS_COST_PER_TRANSFER_NATIVE,
						assets::bsc::Asset::BscUsdt => GAS_COST_PER_TRANSFER_TOKEN,
					};

				self.calculate_transaction_fee(gas_cost_per_transfer)
			},
			IngressOrEgress::EgressCcm { gas_budget, message_length } => {
				let gas_limit = self.calculate_ccm_gas_limit(
					asset == <Bsc as Chain>::GAS_ASSET,
					gas_budget,
					message_length,
				);
				self.calculate_transaction_fee(gas_limit)
			},
		}
	}
}

impl From<&DepositChannel<Bsc>> for EvmFetchId {
	fn from(channel: &DepositChannel<Bsc>) -> Self {
		match channel.state {
			DeploymentStatus::Undeployed => EvmFetchId::DeployAndFetch(channel.channel_id),
			DeploymentStatus::Pending | DeploymentStatus::Deployed =>
				if channel.asset == assets::bsc::Asset::BscBnb {
					EvmFetchId::NotRequired
				} else {
					EvmFetchId::Fetch(channel.address)
				},
		}
	}
}

impl FeeRefundCalculator<Bsc> for evm::Transaction {
	fn return_fee_refund(
		&self,
		fee_paid: <Bsc as Chain>::TransactionFee,
	) -> <Bsc as Chain>::ChainAmount {
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
