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

//! Types and functions that are common to Tron.
pub mod api;

pub mod benchmarking;

// TODO: To revisit those numbers, they are rough estimations.
pub mod fees {
	pub const ENERGY_BASE_COST_PER_BATCH: u128 = 40_000; // Base energy transaction cost
	pub const ENERGY_COST_PER_FETCH_NATIVE: u128 = 5_000; // Cost to fetch tokens from deposit channel
	pub const ENERGY_COST_PER_FETCH_TOKEN: u128 = 70_000; // Cost to fetch tokens from deposit channel
	pub const ENERGY_COST_PER_TRANSFER_NATIVE: u128 = 10_000; // Native TRX transfers included in base
	pub const ENERGY_COST_PER_TRANSFER_TOKEN: u128 = 60_000; // TRC-20 token transfer
	pub const ENERGY_CCM_OVERHEAD: u128 = 45_000;

	// Bandwidth in out case depends on the length. Both types of fetches have the same length
	// and transferring and fetching is the same length for native and token assets.
	pub const BANDWITH_BASE_COST_PER_BATCH: u128 = 268 + 100;
	pub const BANDWITH_BASE_COST_PER_TRANSFER: u128 = 288;
	pub const BANDWITH_BASE_COST_PER_FETCH: u128 = 288;
	pub const BANDWITH_CCM_OVERHEAD: u128 = BANDWITH_BASE_COST_PER_TRANSFER;

	pub const ENERGY_PER_TX_TRX_BURN: u128 = 10_000; // 10_000 Energy per 1 TRX burned as fee
	pub const BANDWIDTH_PER_TX_TRX_BURN: u128 = 1_000; // 0.0001 TRX per 1 Bandwidth
	pub const BANDWIDTH_PER_BYTE: u128 = 1; // 1 Bandwidth per byte of transaction data
	pub const SUN_PER_TRX: u128 = 1_000_000; // 1 TRX = 1_000_000 SUN

	pub const MAX_FEE_LIMIT: u128 = 1_000_000_000; // Currently in mainnet it's 15k TRX but we set it
	                                            // lower to be safe
}

use crate::{
	evm::{DeploymentStatus, EvmFetchId},
	ChainWitnessConfig, FeeEstimationApi, *,
};
use cf_primitives::chains::assets;
pub use cf_primitives::chains::Tron;
use codec::{Decode, Encode, MaxEncodedLen};
pub use ethabi::{
	ethereum_types::{H160, H256},
	Address, Hash as TxHash, Token, Uint, Word,
};
use frame_support::sp_runtime::RuntimeDebug;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::cmp::min;

use self::evm::EvmCrypto;

// Reference constants for the chain spec
pub const CHAIN_ID_MAINNET: u64 = 728126428; // 0x2b6653dc
pub const CHAIN_ID_NILE_TESTNET: u64 = 3448148188; // 0xcd8690dc

// Tron uses i64 so we could use u64. However, using u128 makes it
// simpler to share code with the rest of the EVM-based chains.
pub type TronAmount = u128;

impl ChainWitnessConfig for Tron {
	type ChainBlockNumber = u64;
	const WITNESS_PERIOD: Self::ChainBlockNumber = 1;
}

impl Chain for Tron {
	const NAME: &'static str = "Tron";
	const GAS_ASSET: Self::ChainAsset = assets::tron::Asset::Trx;
	const WITNESS_PERIOD: Self::ChainBlockNumber = 1;
	const FINE_AMOUNT_PER_UNIT: Self::ChainAmount = 1_000_000u128;
	const BURN_ADDRESS: Self::ChainAccount = H160([0; 20]);
	const IS_EVM_CHAIN: bool = true;

	type ChainCrypto = EvmCrypto;
	type ChainBlockNumber = u64;
	type ChainAmount = TronAmount;
	type TransactionFee = TronTransactionFee;
	type TrackedData = TronTrackedData;
	type ChainAsset = assets::tron::Asset;
	type ChainAssetMap<
		T: Member + Parameter + MaxEncodedLen + Copy + BenchmarkValue + FullCodec + Unpin,
	> = assets::tron::AssetMap<T>;
	type ChainAccount = evm::Address;
	type DepositFetchId = EvmFetchId;
	type DepositChannelState = DeploymentStatus;
	type DepositDetails = evm::DepositDetails;
	type Transaction = TronTransaction;
	type TransactionMetadata = TronTransactionMetadata;
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
	DecodeWithMemTracking,
)]
#[codec(mel_bound())]
pub struct TronTrackedData {}

impl Default for TronTrackedData {
	fn default() -> Self {
		frame_support::print("You should not use the default chain tracking, as it's meaningless.");
		Self {}
	}
}

impl TronTrackedData {
	pub fn new() -> Self {
		Self {}
	}

	/// Calculate the fee for a CCM egress transaction
	pub fn calculate_ccm_fee_limit(
		&self,
		is_native_asset: bool,
		gas_budget: AssetAmount,
		message_length: usize,
	) -> <Tron as Chain>::ChainAmount {
		use crate::tron::fees::*;

		let energy =
			ENERGY_CCM_OVERHEAD
				.saturating_add(gas_budget)
				.saturating_add(if is_native_asset {
					ENERGY_COST_PER_TRANSFER_NATIVE
				} else {
					ENERGY_COST_PER_TRANSFER_TOKEN
				});
		let bandwidth = BANDWITH_CCM_OVERHEAD
			.saturating_add((message_length as u128).saturating_mul(BANDWIDTH_PER_BYTE));

		let fee_limit = energy
			.saturating_mul(SUN_PER_TRX)
			.saturating_div(ENERGY_PER_TX_TRX_BURN)
			.saturating_add(
				bandwidth.saturating_mul(SUN_PER_TRX).saturating_div(BANDWIDTH_PER_TX_TRX_BURN),
			);
		fee_limit.min(MAX_FEE_LIMIT)
	}
}

impl FeeEstimationApi<Tron> for TronTrackedData {
	fn estimate_fee(
		&self,
		asset: <Tron as Chain>::ChainAsset,
		ingress_or_egress: IngressOrEgress,
	) -> <Tron as Chain>::ChainAmount {
		use crate::tron::fees::*;

		match ingress_or_egress {
			IngressOrEgress::IngressDepositChannel => {
				let energy = ENERGY_BASE_COST_PER_BATCH +
					match asset {
						assets::tron::Asset::Trx => ENERGY_COST_PER_FETCH_NATIVE,
						assets::tron::Asset::TronUsdt => ENERGY_COST_PER_FETCH_TOKEN,
					};
				let bandwidth = BANDWITH_BASE_COST_PER_BATCH +
					match asset {
						assets::tron::Asset::Trx => BANDWITH_BASE_COST_PER_FETCH,
						assets::tron::Asset::TronUsdt => BANDWITH_BASE_COST_PER_FETCH,
					};

				energy
					.saturating_mul(SUN_PER_TRX)
					.saturating_div(ENERGY_PER_TX_TRX_BURN)
					.saturating_add(
						bandwidth
							.saturating_mul(SUN_PER_TRX)
							.saturating_div(BANDWIDTH_PER_TX_TRX_BURN),
					)
			},
			IngressOrEgress::IngressVaultSwap => 0,
			IngressOrEgress::Egress => {
				let energy = ENERGY_BASE_COST_PER_BATCH +
					match asset {
						assets::tron::Asset::Trx => ENERGY_COST_PER_TRANSFER_NATIVE,
						assets::tron::Asset::TronUsdt => ENERGY_COST_PER_TRANSFER_TOKEN,
					};
				let bandwidth = BANDWITH_BASE_COST_PER_BATCH +
					match asset {
						assets::tron::Asset::Trx => BANDWITH_BASE_COST_PER_TRANSFER,
						assets::tron::Asset::TronUsdt => BANDWITH_BASE_COST_PER_TRANSFER,
					};

				energy
					.saturating_mul(SUN_PER_TRX)
					.saturating_div(ENERGY_PER_TX_TRX_BURN)
					.saturating_add(
						bandwidth
							.saturating_mul(SUN_PER_TRX)
							.saturating_div(BANDWIDTH_PER_TX_TRX_BURN),
					)
			},
			IngressOrEgress::EgressCcm { gas_budget, message_length } => self
				.calculate_ccm_fee_limit(
					asset == assets::tron::Asset::Trx,
					gas_budget,
					message_length,
				),
		}
	}
}

// We'd want to also enforce the fee_limit from the State Chain. However, doing that
// requires instrospection of constructed API transactions to parse the actions within and
// then add up the fee_limit estimation. Instead, we can do similarly to the `gas_limit` in
// Ethereum, let the engines estimate the energy and set the fee_limit. We will only repay
// transactions that have succeeded key verification and that are calls to our contracts.
// We do enforce a `fee_limit` for CCM transactions.
#[derive(
	Encode,
	Decode,
	TypeInfo,
	Clone,
	RuntimeDebug,
	Default,
	PartialEq,
	Eq,
	Serialize,
	Deserialize,
	Ord,
	PartialOrd,
	DecodeWithMemTracking,
)]
pub struct TronTransactionMetadata {
	pub contract: Address,
	pub fee_limit: Option<u64>,
}

impl<C: Chain<Transaction = TronTransaction, TransactionRef = H256>> TransactionMetadata<C>
	for TronTransactionMetadata
{
	fn extract_metadata(transaction: &<C as Chain>::Transaction) -> Self {
		Self { contract: transaction.contract, fee_limit: transaction.fee_limit }
	}

	fn verify_metadata(&self, expected_metadata: &Self) -> bool {
		macro_rules! check_optional {
			($field:ident) => {
				(expected_metadata.$field.is_none() || expected_metadata.$field == self.$field)
			};
		}
		self.contract == expected_metadata.contract && check_optional!(fee_limit)
	}
}

#[derive(
	Clone,
	Debug,
	Default,
	PartialEq,
	Eq,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	Copy,
	Serialize,
	Deserialize,
	PartialOrd,
	Ord,
	DecodeWithMemTracking,
)]
pub struct TronTransactionFee {
	/// Total amount of TRX burned as fee for this transaction in sun
	pub fee: u64,
	/// Amount of Energy consumed in the caller's account
	pub energy_usage: Option<u64>,
	/// Amount of TRX burned to pay for Energy
	pub energy_fee: Option<u64>,
	/// Amount of Energy consumed in the contract deployer's account
	pub origin_energy_usage: Option<u64>,
	/// Total amount of Energy consumed by the transaction
	pub energy_usage_total: Option<u64>,
	/// Amount of Bandwidth consumed
	pub net_usage: Option<u64>,
	/// Amount of TRX burned to pay for Bandwidth
	pub net_fee: Option<u64>,
	/// Amount of extra Energy that needs to be paid for calling a few popular contracts
	pub energy_penalty_total: Option<u64>,
}

/// Required information to construct and sign a TRON transaction.
#[derive(
	Encode,
	Decode,
	TypeInfo,
	Clone,
	RuntimeDebug,
	Default,
	PartialEq,
	Eq,
	Serialize,
	Deserialize,
	DecodeWithMemTracking,
)]
pub struct TronTransaction {
	pub fee_limit: Option<u64>,
	pub contract: Address,
	pub value: Uint,
	#[serde(with = "hex::serde")]
	pub data: Vec<u8>,
	// This is representing a string
	pub function_selector: Vec<u8>,
}

// We don't use all values from the TransactionFee (receipt) but we want to have access to them
// in case we want to use them in the future for better energy estimations.
impl FeeRefundCalculator<Tron> for TronTransaction {
	fn return_fee_refund(
		&self,
		transaction_fees: <Tron as Chain>::TransactionFee,
	) -> <Tron as Chain>::ChainAmount {
		// Calculate total fee paid (energy fee + bandwidth fee). We don't use the total_fee because
		// NOs could add extra fees (e.g. including a note) that we don't really want to refund for.
		let fee_paid = (transaction_fees
			.energy_fee
			.unwrap_or(0)
			.saturating_add(transaction_fees.net_fee.unwrap_or(0))) as u128;

		match self.fee_limit {
			Some(fee_limit) => min(fee_paid, fee_limit.into()),
			None => fee_paid,
		}
	}
}

impl From<&DepositChannel<Tron>> for EvmFetchId {
	fn from(channel: &DepositChannel<Tron>) -> Self {
		use DeploymentStatus::*;
		match channel.state {
			Undeployed => EvmFetchId::DeployAndFetch(channel.channel_id),
			Pending | Deployed { .. } => EvmFetchId::Fetch(channel.address),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn fee_estimation_trx_ingress() {
		let tracked_data = TronTrackedData::new();

		let fee = tracked_data
			.estimate_fee(assets::tron::Asset::Trx, IngressOrEgress::IngressDepositChannel);

		assert_eq!(fee, 6_656_000);
	}

	#[test]
	fn print_ccm_egress_fee_estimates() {
		let tracked_data = TronTrackedData::new();
		let message_length = 500;
		let gas_budget = 50_000u128;

		for (label, asset) in [
			("Trx", assets::tron::Asset::Trx),
			// ("TronUsdt", assets::tron::Asset::TronUsdt),
		] {
			let fee = tracked_data.estimate_fee(
				asset,
				IngressOrEgress::EgressCcm { gas_budget: 0, message_length: 10 },
			);
			println!("{} fee limit only 10 bytes: {}", label, fee);

			let fee = tracked_data
				.estimate_fee(asset, IngressOrEgress::EgressCcm { gas_budget, message_length });
			println!("{} fee limit:               {}", label, fee);
		}
	}
}
