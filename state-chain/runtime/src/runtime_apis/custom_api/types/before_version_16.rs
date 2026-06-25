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
use cf_utilities::migrations::{basics::migrate_from_historical_type, v20000};

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

pub type RpcAccountInfoCommonItems<Balance> =
	<super::RpcAccountInfoCommonItems<Balance> as HasVersion<v20000>>::HistoricalType;

pub type AssetMap<T> = <super::AssetMap<T> as HasVersion<v20000>>::HistoricalType;

#[derive(Encode, Decode, TypeInfo, Default)]
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
			earned_fees: migrate_from_historical_type(v20000, value.earned_fees),
			boost_balances: migrate_from_historical_type(v20000, value.boost_balances),
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
			minimum_deployment_amount: migrate_from_historical_type(
				v20000,
				value.minimum_deployment_amount,
			),
			minimum_added_funds_amount: migrate_from_historical_type(
				v20000,
				value.minimum_added_funds_amount,
			),
		}
	}
}

pub type NetworkFees = <super::NetworkFees as HasVersion<v20000>>::HistoricalType;
