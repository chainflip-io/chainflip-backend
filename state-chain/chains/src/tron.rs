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

// TODO: To update/change.
pub mod fees {
	pub const ENERGY_BASE_COST_PER_BATCH: u128 = 21_000; // Base energy transaction cost
	pub const ENERGY_GAS_COST_PER_FETCH_NATIVE: u128 = 65_000; // Cost to fetch tokens from deposit channel
	pub const ENERGY_COST_PER_FETCH_TOKEN: u128 = 65_000; // Cost to fetch tokens from deposit channel
	pub const ENERGY_COST_PER_TRANSFER_NATIVE: u128 = 0; // Native TRX transfers included in base
	pub const ENERGY_COST_PER_TRANSFER_TOKEN: u128 = 45_000; // TRC-20 token transfer

	// Bandwidth in out case depends on the length. Both types of fetches have the same length
	//  and transferring and fetching is the same length for native and token assets.
	pub const BANDWITH_BASE_COST_PER_BATCH: u128 = 5_000;
	pub const BANDWITH_BASE_COST_PER_TRANSFER: u128 = 5_000;
	pub const BANDWITH_BASE_COST_PER_FETCH: u128 = 5_000;

	pub const ENERGY_PER_TX_TRX_BURN: u128 = 420; // 0.00042 TRX per 1 Energy
	pub const BANDWIDTH_PER_TX_TRX_BURN: u128 = 345000; // 0.0001 TRX per 1 Bandwidth
	pub const BANDWIDTH_PER_BYTE: u128 = 1; // 1 Bandwidth per byte of transaction data
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

	type ChainCrypto = EvmCrypto;
	type ChainBlockNumber = u64;
	type ChainAmount = TronAmount;
	type TransactionFee = TronTransactionFee;
	type TrackedData = TronTrackedData;
	type ChainAsset = assets::tron::Asset;
	type ChainAssetMap<
		T: Member + Parameter + MaxEncodedLen + Copy + BenchmarkValue + FullCodec + Unpin,
	> = assets::tron::AssetMap<T>;
	type ChainAccount = eth::Address;
	type DepositFetchId = EvmFetchId;
	type DepositChannelState = DeploymentStatus;
	type DepositDetails = evm::DepositDetails; // TODO: To update??
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
)]
#[codec(mel_bound())]
pub struct TronTrackedData {}

impl Default for TronTrackedData {
	#[track_caller]
	fn default() -> Self {
		frame_support::print("You should not use the default chain tracking, as it's meaningless.");
		Self {}
	}
}

impl TronTrackedData {
	pub fn new() -> Self {
		Self {}
	}
}

// TODO: To review. This currently will return the worst case scenario cost-wise - all energy
// and bandwhidth are paid with burning TRX on transaction inclusion. This approach plus using
// the governance fee multiplier is one alternative. It can then be adjusted depending on how
// much bandwidth and energy we have available. The alternative is to track how much energy
// we have available in chaintracking.
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
						assets::tron::Asset::Trx => ENERGY_GAS_COST_PER_FETCH_NATIVE,
						assets::tron::Asset::TronUsdt => ENERGY_COST_PER_FETCH_TOKEN,
					};
				let bandwidth = BANDWITH_BASE_COST_PER_BATCH +
					match asset {
						assets::tron::Asset::Trx => BANDWITH_BASE_COST_PER_FETCH,
						assets::tron::Asset::TronUsdt => BANDWITH_BASE_COST_PER_FETCH,
					};

				energy
					.saturating_mul(ENERGY_PER_TX_TRX_BURN)
					.saturating_add(bandwidth.saturating_mul(BANDWIDTH_PER_TX_TRX_BURN))
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
					.saturating_mul(ENERGY_PER_TX_TRX_BURN)
					.saturating_add(bandwidth.saturating_mul(BANDWIDTH_PER_TX_TRX_BURN))
			},
			IngressOrEgress::EgressCcm { gas_budget, message_length } => {
				let energy = ENERGY_BASE_COST_PER_BATCH.saturating_add(gas_budget) +
					match asset {
						assets::tron::Asset::Trx => ENERGY_COST_PER_TRANSFER_NATIVE,
						assets::tron::Asset::TronUsdt => ENERGY_COST_PER_TRANSFER_TOKEN,
					};
				let bandwidth = BANDWITH_BASE_COST_PER_BATCH +
					match asset {
						assets::tron::Asset::Trx => BANDWITH_BASE_COST_PER_TRANSFER,
						assets::tron::Asset::TronUsdt => BANDWITH_BASE_COST_PER_TRANSFER,
					} + (message_length as u128).saturating_mul(BANDWIDTH_PER_BYTE);

				energy
					.saturating_mul(ENERGY_PER_TX_TRX_BURN)
					.saturating_add(bandwidth.saturating_mul(BANDWIDTH_PER_TX_TRX_BURN))
			},
		}
	}
}

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
)]
pub struct TronTransactionMetadata {
	// pub max_fee_per_gas: Option<Uint>,
	// pub max_priority_fee_per_gas: Option<Uint>,
	// pub gas_limit: Option<Uint>,
	pub contract: Address,
	pub fee_limit: Option<Uint>,
	// Depending on how we end up implementing the fee charging, we
	// might end up with a user paying only part of the egresses costs.
	// For normal transactions that's fine. For CCM, where the user
	// sets the budget, this could be a problem. Energy consumption
	// can't be set in the transaction and limiting the final fee is
	// not reliable if we have a lot of energy available - CCM could
	// then be used to "drain" a lot of the energy. Therefore,
	// we could consider using a max_energy in the meteadata for the
	// engines to estimate the energy before actuallly broadcasting it,
	// similar to the gas_limit in other EVM-based chains. We shall
	// then use the `triggerConstantSmartContract` api call.
	// Alternatively or parallely we could decide to charg the user
	// in full for CCM transacctions as if the whole tx energy and
	// bandwith was paid by burning TRX. This way we can't end up in
	// a deficit.
	// pub max_energy: Option<Uint>,
}

// TODO: To update/review
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

		// TODO: Fee_limit is optional?
		self.contract == expected_metadata.contract && check_optional!(fee_limit)
	}
}

// TODO: TBD
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
)]
pub struct TronTransactionFee {
	/// Total amount of TRX burned as fee for this transaction in sun
	pub fee: u64,
	/// Amount of Energy consumed in the caller's account
	pub energy_usage: u64,
	/// Amount of TRX burned to pay for Energy
	pub energy_fee: u64,
	/// Amount of Energy consumed in the contract deployer's account
	pub origin_energy_usage: u64,
	/// Total amount of Energy consumed by the transaction
	pub energy_usage_total: u64,
	/// Amount of Bandwidth consumed
	pub net_usage: u64,
	/// Amount of TRX burned to pay for Bandwidth
	pub net_fee: u64,
	/// Amount of extra Energy that needs to be paid for calling a few popular contracts
	pub energy_penalty_total: u64,
}

// TODO: To update/review
/// Required information to construct and sign a TRON transaction.
#[derive(
	Encode, Decode, TypeInfo, Clone, RuntimeDebug, Default, PartialEq, Eq, Serialize, Deserialize,
)]
pub struct TronTransaction {
	pub chain_id: u64,
	pub fee_limit: Option<Uint>,
	pub contract: Address,
	pub value: Uint,
	#[serde(with = "hex::serde")]
	pub data: Vec<u8>,
}

impl FeeRefundCalculator<Tron> for TronTransaction {
	fn return_fee_refund(
		&self,
		fee_paid: <Tron as Chain>::TransactionFee,
	) -> <Tron as Chain>::ChainAmount {
		// TODO: To set and/or calculate the fee we want to refund back. For now we implement
		// the simplest possible approach - pay back for energy and bandwidth. This has the
		// edge case of the transaction having activated an account (extra 1 TRX). There is no
		// incentive to abuse it (unlike Solana) but we might want to consider it e.g. actually
		// use the total fee instead.

		// Calculate total fee paid (energy_fee + net_fee)
		let fee_paid = (fee_paid.energy_fee.saturating_add(fee_paid.net_fee)) as u128;

		// Limit the refund by fee_limit if it exists
		if let Some(fee_limit) = self.fee_limit {
			let fee_limit_u128: u128 = fee_limit.try_into().unwrap_or(u128::MAX);
			min(fee_paid, fee_limit_u128)
		} else {
			fee_paid
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
