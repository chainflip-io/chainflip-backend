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

// --- TronAddress ---

extern crate alloc;
use alloc::{format, string::String, vec::Vec};
use sha2::{Digest, Sha256};

const TRON_PREFIX_BYTE: u8 = 0x41;

fn double_sha256(data: &[u8]) -> [u8; 32] {
	let hash1 = Sha256::digest(data);
	Sha256::digest(hash1).into()
}

/// A Tron address: 0x41 prefix + 20-byte EVM address.
///
/// Display/FromStr use base58check encoding (always starts with 'T').
/// Serde uses hex encoding (42 hex chars: "41" + 40 hex digits) for Tron HTTP API compatibility.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TronAddress(pub [u8; 21]);

impl TronAddress {
	pub fn from_evm_address(evm_address: Address) -> Self {
		let mut tron_address = [0u8; 21];
		tron_address[0] = TRON_PREFIX_BYTE;
		tron_address[1..].copy_from_slice(evm_address.as_bytes());
		TronAddress(tron_address)
	}

	pub fn to_evm_address(&self) -> Result<Address, &'static str> {
		if self.0[0] != TRON_PREFIX_BYTE {
			return Err("Invalid Tron address: expected 0x41 prefix");
		}
		Ok(Address::from_slice(&self.0[1..]))
	}

	/// Encode as a base58check string.
	pub fn to_base58check(&self) -> String {
		let checksum = double_sha256(&self.0);
		let mut with_checksum = Vec::with_capacity(25);
		with_checksum.extend_from_slice(&self.0);
		with_checksum.extend_from_slice(&checksum[..4]);
		bs58::encode(with_checksum).into_string()
	}

	/// Decode from a base58check string.
	pub fn from_base58check(address: &str) -> Result<Self, &'static str> {
		let bytes = bs58::decode(address).into_vec().map_err(|_| "Invalid base58")?;

		if bytes.len() != 25 {
			return Err("Invalid Tron address length");
		}

		if bytes[0] != TRON_PREFIX_BYTE {
			return Err("Invalid Tron version byte");
		}

		let checksum = double_sha256(&bytes[..21]);
		if bytes[21..] != checksum[..4] {
			return Err("Invalid Tron address checksum");
		}

		let mut inner = [0u8; 21];
		inner.copy_from_slice(&bytes[..21]);
		Ok(TronAddress(inner))
	}
}

impl TryFrom<Vec<u8>> for TronAddress {
	type Error = &'static str;

	fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
		let inner: [u8; 21] =
			bytes.try_into().map_err(|_| "Invalid Tron address: expected 21 bytes")?;
		if inner[0] != TRON_PREFIX_BYTE {
			return Err("Invalid Tron address: expected 0x41 prefix");
		}
		Ok(TronAddress(inner))
	}
}

impl core::str::FromStr for TronAddress {
	type Err = &'static str;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Self::from_base58check(s)
	}
}

impl core::fmt::Display for TronAddress {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		write!(f, "{}", self.to_base58check())
	}
}

impl Serialize for TronAddress {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		serializer.serialize_str(&hex::encode(self.0))
	}
}

impl<'de> Deserialize<'de> for TronAddress {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		let s = <String as Deserialize>::deserialize(deserializer)?;
		let hex_str = s.strip_prefix("0x").unwrap_or(&s);

		if hex_str.len() != 42 {
			return Err(serde::de::Error::custom(format!(
				"Invalid hex length: expected 42, got {}",
				hex_str.len()
			)));
		}

		let bytes = hex::decode(hex_str)
			.map_err(|e| serde::de::Error::custom(format!("Failed to decode hex: {e}")))?;

		let mut address = [0u8; 21];
		address.copy_from_slice(&bytes);
		Ok(TronAddress(address))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use core::str::FromStr;

	#[test]
	fn fee_estimation_trx_ingress() {
		let tracked_data = TronTrackedData::new();

		let fee = tracked_data
			.estimate_fee(assets::tron::Asset::Trx, IngressOrEgress::IngressDepositChannel);

		assert_eq!(fee, 5_156_000);
	}

	#[test]
	fn base58check_roundtrip() {
		let base58 = "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t";
		let tron_addr = TronAddress::from_base58check(base58).unwrap();
		assert_eq!(tron_addr.to_base58check(), base58);
	}

	#[test]
	fn from_str_roundtrip() {
		let base58 = "TJRabPrwbZy45sbavfcjinPJC18kjpRTv8";
		let tron_addr = TronAddress::from_str(base58).unwrap();
		assert_eq!(tron_addr.to_string(), base58);
	}

	#[test]
	fn evm_address_roundtrip() {
		let evm_addr = Address::from([0xABu8; 20]);
		let tron_addr = TronAddress::from_evm_address(evm_addr);
		assert_eq!(tron_addr.to_evm_address().unwrap(), evm_addr);
		assert_eq!(tron_addr.0[0], 0x41);
	}

	#[test]
	fn base58check_to_evm_bytes() {
		let base58 = "TQNPGpohiZLiWQvc6wTWjHCae8VoxaXnej";
		let tron_addr = TronAddress::from_base58check(base58).unwrap();
		let evm = tron_addr.to_evm_address().unwrap();
		assert_eq!(hex::encode(evm.as_bytes()), "9df3e70fc7ea8128d6d0634664118d16bc856e1c");
	}

	#[test]
	fn evm_bytes_to_base58check() {
		let evm_hex = "9df3e70fc7ea8128d6d0634664118d16bc856e1c";
		let evm_bytes: [u8; 20] = hex::decode(evm_hex).unwrap().try_into().unwrap();
		let tron_addr = TronAddress::from_evm_address(Address::from(evm_bytes));
		assert_eq!(tron_addr.to_base58check(), "TQNPGpohiZLiWQvc6wTWjHCae8VoxaXnej");
	}

	#[test]
	fn invalid_base58check_rejected() {
		// Wrong checksum
		assert!(TronAddress::from_base58check("TJRabPrwbZy45sbavfcjinPJC18kjpRTv9").is_err());
		// Too short
		assert!(TronAddress::from_base58check("T").is_err());
		// Not base58
		assert!(TronAddress::from_base58check("0000").is_err());
	}

	#[test]
	fn try_from_hex_bytes() {
		// Valid: 0x41 prefix + 20 bytes
		let mut bytes = vec![0x41u8];
		bytes.extend_from_slice(&[0xAB; 20]);
		let tron_addr = TronAddress::try_from(bytes).unwrap();
		assert_eq!(tron_addr.to_evm_address().unwrap(), Address::from([0xAB; 20]));

		// Invalid: wrong prefix
		let mut bad_prefix = vec![0x00u8];
		bad_prefix.extend_from_slice(&[0xAB; 20]);
		assert!(TronAddress::try_from(bad_prefix).is_err());

		// Invalid: wrong length
		assert!(TronAddress::try_from(vec![0x41, 0x01, 0x02]).is_err());
	}
}

#[cfg(test)]
mod lifecycle_tests {
	use super::*;
	use crate::ChannelLifecycleHooks;

	const TRX: assets::tron::Asset = assets::tron::Asset::Trx;
	const USDT: assets::tron::Asset = assets::tron::Asset::TronUsdt;

	macro_rules! expect_deposit_state {
		( $state:expr, $asset:expr, $pat:pat ) => {
			cf_utilities::assert_matches!(
				DepositChannel::<Tron> {
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
	fn tron_deposit_address_lifecycle() {
		const TEST_BLOCK_NUMBER: u64 = 42;

		// Initial state is undeployed.
		let mut state = DeploymentStatus::default();
		assert_eq!(state, DeploymentStatus::Undeployed);
		assert!(state.can_fetch());
		expect_deposit_state!(state, TRX, EvmFetchId::DeployAndFetch(..));
		expect_deposit_state!(state, USDT, EvmFetchId::DeployAndFetch(..));

		// Pending channels can't be fetched from.
		assert!(state.on_fetch_scheduled());
		assert_eq!(state, DeploymentStatus::Pending);
		assert!(!state.can_fetch());

		// Trying to schedule the fetch on a pending channel has no effect.
		assert!(!state.on_fetch_scheduled());
		assert_eq!(state, DeploymentStatus::Pending);
		assert!(!state.can_fetch());

		// On completion, the pending channel is now deployed and can be fetched from again.
		assert!(state.on_fetch_completed(TEST_BLOCK_NUMBER));
		assert_eq!(state, DeploymentStatus::Deployed { at_block_height: TEST_BLOCK_NUMBER });
		assert!(state.can_fetch());

		// Both native TRX and any ERC-20 require a fetch. This is a Tron-specific behaviour,
		// in other EVMs the native asset does not require a fetch after deployment.
		expect_deposit_state!(state, TRX, EvmFetchId::Fetch(..));
		expect_deposit_state!(state, USDT, EvmFetchId::Fetch(..));

		// Channel is now in its final deployed state and can be fetched from at any time.
		assert!(!state.on_fetch_scheduled());
		assert!(state.can_fetch());
		assert!(!state.on_fetch_completed(TEST_BLOCK_NUMBER + 1));
		assert!(state.can_fetch());
		expect_deposit_state!(state, TRX, EvmFetchId::Fetch(..));
		expect_deposit_state!(state, USDT, EvmFetchId::Fetch(..));

		assert_eq!(state, DeploymentStatus::Deployed { at_block_height: TEST_BLOCK_NUMBER });
		assert!(!state.on_fetch_scheduled());
	}
}
