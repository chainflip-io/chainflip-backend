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

use cf_amm::common::PoolPairsMap;
use cf_chains::assets::any::AssetMap;
use cf_primitives::{Asset, AssetAmount};
use frame_support::pallet_prelude::{DispatchError, DispatchResult};
use sp_std::{vec, vec::Vec};

pub trait LpDepositHandler {
	type AccountId;

	/// Attempt to credit the account with the given asset and amount
	/// as a result of a liquidity deposit.
	fn add_deposit(who: &Self::AccountId, asset: Asset, amount: AssetAmount) -> DispatchResult;
}

/// API for interacting with the liquidity provider pallet.
pub trait LpRegistration {
	type AccountId;

	/// Register an address for an given account. This is for benchmarking purposes only.
	#[cfg(feature = "runtime-benchmarks")]
	fn register_liquidity_refund_address(
		who: &Self::AccountId,
		address: cf_chains::ForeignChainAddress,
	);

	/// Ensure that the given account has a refund address set for the given asset.
	fn ensure_has_refund_address_for_asset(who: &Self::AccountId, asset: Asset) -> DispatchResult;
}

pub trait HistoricalFeeMigration {
	type AccountId;
	fn migrate_historical_fee(account_id: Self::AccountId, asset: Asset, amount: AssetAmount);
	fn get_fee_amount(account_id: Self::AccountId, asset: Asset) -> AssetAmount;
}

pub trait PoolApi {
	type AccountId;

	/// Sweep all earnings of an LP into their free balance (Should be called before any assets are
	/// debited from their free balance)
	fn sweep(who: &Self::AccountId) -> Result<(), DispatchError>;

	/// Returns the number of open orders for the given account and pair.
	fn open_order_count(
		who: &Self::AccountId,
		asset_pair: &PoolPairsMap<Asset>,
	) -> Result<u32, DispatchError>;

	fn open_order_balances(who: &Self::AccountId) -> AssetMap<AssetAmount>;

	fn pools() -> Vec<PoolPairsMap<Asset>>;
}

impl<T: frame_system::Config> PoolApi for T {
	type AccountId = T::AccountId;

	fn sweep(_who: &Self::AccountId) -> Result<(), DispatchError> {
		Ok(())
	}

	fn open_order_count(
		_who: &Self::AccountId,
		_asset_pair: &PoolPairsMap<Asset>,
	) -> Result<u32, DispatchError> {
		Ok(0)
	}
	fn open_order_balances(_who: &Self::AccountId) -> AssetMap<AssetAmount> {
		AssetMap::from_fn(|_| 0)
	}
	fn pools() -> Vec<PoolPairsMap<Asset>> {
		vec![]
	}
}

pub trait SwappingApi {
	/// Process a single leg of a swap, into or from Stable asset. No network fee is taken.
	fn swap_single_leg(
		from: Asset,
		to: Asset,
		input_amount: AssetAmount,
	) -> Result<AssetAmount, DispatchError>;
}

pub trait BoostApi {
	type AccountId;
	type AssetMap;

	fn boost_pool_account_balances(who: &Self::AccountId) -> Self::AssetMap;
}
