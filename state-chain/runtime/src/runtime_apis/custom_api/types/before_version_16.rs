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
use super::*;
use cf_utilities::migrations::{v20000, v20100, v20200, v20300};
use codec::{DecodeWithMemTracking, MaxEncodedLen};

#[derive(Encode, Decode, TypeInfo, Clone)]
pub struct VaultAddresses {
	pub ethereum: EncodedAddress,
	pub arbitrum: EncodedAddress,
	pub bitcoin: Vec<(AccountId32, EncodedAddress)>,
	pub sol_vault_program: EncodedAddress,
	pub sol_swap_endpoint_program_data_account: EncodedAddress,
	pub usdc_token_mint_pubkey: EncodedAddress,

	pub bitcoin_vault: Option<EncodedAddress>,
	pub solana_sol_vault: Option<EncodedAddress>,
	pub solana_usdc_token_vault_ata: EncodedAddress,
	pub solana_vault_swap_account: Option<EncodedAddress>,

	pub predicted_seconds_until_next_vault_rotation: u64,
}

impl From<VaultAddresses> for super::VaultAddresses {
	fn from(old: VaultAddresses) -> Self {
		Self {
			ethereum: old.ethereum,
			arbitrum: old.arbitrum,
			bitcoin: old.bitcoin,
			sol_vault_program: old.sol_vault_program,
			sol_swap_endpoint_program_data_account: old.sol_swap_endpoint_program_data_account,
			usdc_token_mint_pubkey: old.usdc_token_mint_pubkey,
			bitcoin_vault: old.bitcoin_vault,
			solana_sol_vault: old.solana_sol_vault,
			solana_usdc_token_vault_ata: old.solana_usdc_token_vault_ata,
			solana_vault_swap_account: old.solana_vault_swap_account,
			tron: EncodedAddress::Tron([0u8; 20]),
			predicted_seconds_until_next_vault_rotation: old
				.predicted_seconds_until_next_vault_rotation,
			// Set usdt token pubkey and ata to null addresses
			usdt_token_mint_pubkey: EncodedAddress::Sol([0u8; 32]),
			solana_usdt_token_vault_ata: EncodedAddress::Sol([0u8; 32]),
		}
	}
}

pub type RpcAccountInfoCommonItems<Balance: HasChangelog + Default>
	= <super::RpcAccountInfoCommonItems<Balance> as HasVersion<v20000>>::HistoricalType
where
	<Balance as HasVersion<v20300>>::HistoricalType: Default,
	<Balance as HasVersion<v20200>>::HistoricalType: Default,
	<Balance as HasVersion<v20100>>::HistoricalType: Default;

#[derive(
	Copy,
	Clone,
	Debug,
	PartialEq,
	Eq,
	Hash,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
	Default,
)]
pub struct AssetMap<T> {
	pub eth: EthAssetMap<T>,
	pub dot: cf_primitives::chains::assets::dot::AssetMap<T>,
	pub btc: cf_primitives::chains::assets::btc::AssetMap<T>,
	pub arb: ArbAssetMap<T>,
	pub sol: SolAssetMap<T>,
	pub hub: cf_primitives::chains::assets::hub::AssetMap<T>,
}

#[derive(
	Copy,
	Clone,
	Debug,
	PartialEq,
	Eq,
	Hash,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
	Default,
)]
pub struct EthAssetMap<T> {
	pub eth: T,
	pub flip: T,
	pub usdc: T,
	pub usdt: T,
}
impl<T: Default> From<EthAssetMap<T>> for cf_primitives::chains::assets::eth::AssetMap<T> {
	fn from(value: EthAssetMap<T>) -> Self {
		Self {
			eth: value.eth,
			flip: value.flip,
			usdc: value.usdc,
			usdt: value.usdt,
			wbtc: T::default(),
		}
	}
}

#[derive(
	Copy,
	Clone,
	Debug,
	PartialEq,
	Eq,
	Hash,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
	Default,
)]
pub struct ArbAssetMap<T> {
	pub eth: T,
	pub usdc: T,
}
impl<T: Default> From<ArbAssetMap<T>> for cf_primitives::chains::assets::arb::AssetMap<T> {
	fn from(value: ArbAssetMap<T>) -> Self {
		Self { eth: value.eth, usdc: value.usdc, usdt: T::default() }
	}
}

#[derive(
	Copy,
	Clone,
	Debug,
	PartialEq,
	Eq,
	Hash,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
	Default,
)]
pub struct SolAssetMap<T> {
	pub sol: T,
	pub usdc: T,
}
impl<T: Default> From<SolAssetMap<T>> for cf_primitives::chains::assets::sol::AssetMap<T> {
	fn from(value: SolAssetMap<T>) -> Self {
		Self { sol: value.sol, usdc: value.usdc, usdt: T::default() }
	}
}

impl<T: Default> From<AssetMap<T>> for cf_primitives::chains::assets::any::AssetMap<T> {
	fn from(value: AssetMap<T>) -> Self {
		Self {
			eth: value.eth.into(),
			dot: value.dot,
			btc: value.btc,
			arb: value.arb.into(),
			sol: value.sol.into(),
			hub: value.hub,
			tron: Default::default(),
		}
	}
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo, Default)]
pub struct LiquidityProviderInfo {
	pub refund_addresses: Vec<(ForeignChain, Option<ForeignChainAddress>)>,
	pub balances: Vec<(Asset, AssetAmount)>,
	pub earned_fees: AssetMap<AssetAmount>,
	pub boost_balances: AssetMap<Vec<LiquidityProviderBoostPoolInfo>>,
	pub lending_positions: Vec<LendingPosition<AssetAmount>>,
	pub collateral_balances: Vec<(Asset, AssetAmount)>,
}

impl From<LiquidityProviderInfo> for super::LiquidityProviderInfo {
	fn from(value: LiquidityProviderInfo) -> Self {
		Self {
			refund_addresses: value.refund_addresses,
			balances: value.balances,
			earned_fees: value.earned_fees.into(),
			boost_balances: value.boost_balances.into(),
			lending_positions: value.lending_positions,
			collateral_balances: value.collateral_balances,
		}
	}
}

#[derive(Encode, Decode, TypeInfo, Clone)]
pub struct TradingStrategyLimits {
	pub minimum_deployment_amount: AssetMap<Option<AssetAmount>>,
	pub minimum_added_funds_amount: AssetMap<Option<AssetAmount>>,
}

impl From<TradingStrategyLimits> for super::TradingStrategyLimits {
	fn from(value: TradingStrategyLimits) -> Self {
		Self {
			minimum_deployment_amount: value.minimum_deployment_amount.into(),
			minimum_added_funds_amount: value.minimum_added_funds_amount.into(),
		}
	}
}

pub type NetworkFees = <super::NetworkFees as HasVersion<v20000>>::HistoricalType;
