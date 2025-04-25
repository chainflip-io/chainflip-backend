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

pub use cf_amm::common::{PoolPairsMap, Side};
use cf_chains::assets::any::AssetMap;
use cf_primitives::{Asset, AssetAmount, Tick};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::{DispatchError, DispatchResult};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::vec::Vec;

pub type OrderId = u64;

/// Indicates if an LP wishes to increase or decrease the size of an order.
#[derive(
	Copy,
	Clone,
	Debug,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	PartialEq,
	Eq,
	Deserialize,
	Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum IncreaseOrDecrease<T> {
	Increase(T),
	Decrease(T),
}

impl<T> IncreaseOrDecrease<T> {
	pub fn abs(&self) -> &T {
		match self {
			IncreaseOrDecrease::Increase(t) => t,
			IncreaseOrDecrease::Decrease(t) => t,
		}
	}

	pub fn map<R, F: FnOnce(T) -> R>(self, f: F) -> IncreaseOrDecrease<R> {
		match self {
			IncreaseOrDecrease::Increase(t) => IncreaseOrDecrease::Increase(f(t)),
			IncreaseOrDecrease::Decrease(t) => IncreaseOrDecrease::Decrease(f(t)),
		}
	}

	pub fn try_map<R, E, F: FnOnce(T) -> Result<R, E>>(
		self,
		f: F,
	) -> Result<IncreaseOrDecrease<R>, E> {
		Ok(match self {
			IncreaseOrDecrease::Increase(t) => IncreaseOrDecrease::Increase(f(t)?),
			IncreaseOrDecrease::Decrease(t) => IncreaseOrDecrease::Decrease(f(t)?),
		})
	}
}

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

	/// Ensure that the given account has a refund address set for the given assets.
	fn ensure_has_refund_address_for_assets(
		who: &Self::AccountId,
		assets: impl IntoIterator<Item = Asset>,
	) -> DispatchResult {
		for asset in assets {
			Self::ensure_has_refund_address_for_asset(who, asset)?;
		}
		Ok(())
	}
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

	fn update_limit_order(
		account: &Self::AccountId,
		base_asset: Asset,
		quote_asset: Asset,
		side: Side,
		id: OrderId,
		option_tick: Option<Tick>,
		amount_change: IncreaseOrDecrease<AssetAmount>,
	) -> DispatchResult;

	fn cancel_all_limit_orders(account: &Self::AccountId) -> DispatchResult;

	fn cancel_limit_order(
		account: &Self::AccountId,
		base_asset: Asset,
		quote_asset: Asset,
		side: Side,
		id: OrderId,
		tick: Tick,
	) -> DispatchResult {
		Self::update_limit_order(
			account,
			base_asset,
			quote_asset,
			side,
			id,
			Some(tick),
			IncreaseOrDecrease::Decrease(AssetAmount::MAX),
		)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn create_pool(
		base_asset: Asset,
		quote_asset: Asset,
		fee_hundredth_pips: u32,
		initial_price: cf_primitives::Price,
	) -> DispatchResult;
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
