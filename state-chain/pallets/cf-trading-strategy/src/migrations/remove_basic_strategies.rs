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

//! Closes all `TickZeroCentered` and `SimpleBuySell` strategies, returning their funds to their
//! owners, since those strategy types no longer exist.

use crate::{Config, Pallet, Strategies};
use cf_primitives::{Asset, STABLE_ASSET};
use cf_runtime_utilities::log_or_panic;
use frame_support::{
	storage::with_storage_layer, traits::UncheckedOnRuntimeUpgrade, weights::Weight,
};
use sp_std::{collections::btree_set::BTreeSet, marker::PhantomData, vec::Vec};

use crate::weights::WeightInfo;
use frame_support::traits::Get;

#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::DispatchError;

pub mod old {
	use crate::{Config, Pallet};
	use cf_primitives::{Asset, Tick};
	use codec::{Decode, Encode};
	use frame_support::{storage_alias, Identity};

	#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq)]
	pub enum TradingStrategy {
		TickZeroCentered {
			spread_tick: Tick,
			base_asset: Asset,
		},
		SimpleBuySell {
			buy_tick: Tick,
			sell_tick: Tick,
			base_asset: Asset,
		},
		InventoryBased {
			min_buy_tick: Tick,
			max_buy_tick: Tick,
			min_sell_tick: Tick,
			max_sell_tick: Tick,
			base_asset: Asset,
		},
		OracleTracking {
			min_buy_offset_tick: Tick,
			max_buy_offset_tick: Tick,
			min_sell_offset_tick: Tick,
			max_sell_offset_tick: Tick,
			base_asset: Asset,
			quote_asset: Asset,
		},
	}

	impl TradingStrategy {
		/// The base asset of a removed strategy type, or `None` for strategies that remain.
		pub fn removed_base_asset(&self) -> Option<Asset> {
			match self {
				TradingStrategy::TickZeroCentered { base_asset, .. } |
				TradingStrategy::SimpleBuySell { base_asset, .. } => Some(*base_asset),
				TradingStrategy::InventoryBased { .. } | TradingStrategy::OracleTracking { .. } =>
					None,
			}
		}
	}

	#[storage_alias]
	pub type Strategies<T: Config> = StorageDoubleMap<
		Pallet<T>,
		Identity,
		<T as frame_system::Config>::AccountId,
		Identity,
		<T as frame_system::Config>::AccountId,
		TradingStrategy,
	>;
}

pub struct Migration<T>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		let mut total: u64 = 0;
		let to_close: Vec<_> = old::Strategies::<T>::iter()
			.inspect(|_| total += 1)
			.filter_map(|(lp, strategy_id, strategy)| {
				strategy.removed_base_asset().map(|base_asset| (lp, strategy_id, base_asset))
			})
			.collect();
		let closed = to_close.len() as u64;

		for (lp, strategy_id, base_asset) in to_close {
			Strategies::<T>::remove(&lp, &strategy_id);
			let assets = BTreeSet::<Asset>::from_iter([base_asset, STABLE_ASSET]);
			if let Err(e) =
				with_storage_layer(|| Pallet::<T>::wind_down_strategy(&lp, &strategy_id, assets))
			{
				log_or_panic!(
					"Failed to close removed strategy {:?} of LP {:?}, its funds remain in the strategy account: {:?}",
					strategy_id,
					lp,
					e
				);
			}
		}

		T::DbWeight::get()
			.reads(total)
			.saturating_add(T::WeightInfo::close_strategy().saturating_mul(closed))
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let remaining = old::Strategies::<T>::iter_values()
			.filter(|strategy| strategy.removed_base_asset().is_none())
			.count() as u32;
		Ok(remaining.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let remaining = u32::decode(&mut &state[..])
			.map_err(|_| DispatchError::Other("Failed to decode pre-upgrade state"))?;
		// Counting keys and decoded values separately catches any entry that no longer decodes.
		frame_support::ensure!(
			Strategies::<T>::iter_keys().count() as u32 == remaining,
			"Unexpected number of strategies after migration"
		);
		frame_support::ensure!(
			Strategies::<T>::iter_values().count() as u32 == remaining,
			"Some strategies no longer decode"
		);
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{mock::*, TradingStrategy};
	use cf_traits::{
		mocks::{balance_api::MockBalance, pool_api::MockPoolApi},
		BalanceApi, PoolApi, Side,
	};

	const OWNER: u64 = LP;
	const BASIC_STRATEGY_ID: u64 = 100;
	const SIMPLE_STRATEGY_ID: u64 = 101;
	const INVENTORY_STRATEGY_ID: u64 = 102;
	const AMOUNT: u128 = 1_000;

	#[test]
	fn closes_removed_strategies_and_keeps_the_rest() {
		new_test_ext().execute_with(|| {
			let base_asset = Asset::Usdt;
			old::Strategies::<Test>::insert(
				OWNER,
				BASIC_STRATEGY_ID,
				old::TradingStrategy::TickZeroCentered { spread_tick: 1, base_asset },
			);
			old::Strategies::<Test>::insert(
				OWNER,
				SIMPLE_STRATEGY_ID,
				old::TradingStrategy::SimpleBuySell { buy_tick: -1, sell_tick: 1, base_asset },
			);
			old::Strategies::<Test>::insert(
				OWNER,
				INVENTORY_STRATEGY_ID,
				old::TradingStrategy::InventoryBased {
					min_buy_tick: -1,
					max_buy_tick: 0,
					min_sell_tick: 0,
					max_sell_tick: 1,
					base_asset,
				},
			);
			for strategy_id in [BASIC_STRATEGY_ID, SIMPLE_STRATEGY_ID, INVENTORY_STRATEGY_ID] {
				MockBalance::credit_account(&strategy_id, base_asset, AMOUNT);
				MockBalance::credit_account(&strategy_id, STABLE_ASSET, AMOUNT);
			}
			assert!(MockPoolApi::update_limit_order(
				&BASIC_STRATEGY_ID,
				base_asset,
				STABLE_ASSET,
				Side::Sell,
				0,
				Some(1),
				cf_traits::IncreaseOrDecrease::Increase(AMOUNT),
			)
			.is_ok());

			Migration::<Test>::on_runtime_upgrade();

			// The remaining strategy is untouched and decodes as the new type.
			assert_eq!(
				Strategies::<Test>::iter().collect::<Vec<_>>(),
				vec![(
					OWNER,
					INVENTORY_STRATEGY_ID,
					TradingStrategy::InventoryBased {
						min_buy_tick: -1,
						max_buy_tick: 0,
						min_sell_tick: 0,
						max_sell_tick: 1,
						base_asset,
					}
				)]
			);
			assert_eq!(MockBalance::get_balance(&INVENTORY_STRATEGY_ID, base_asset), AMOUNT);

			// The removed strategies have no orders left and their funds went back to the owner.
			assert!(MockPoolApi::<u64>::get_limit_orders()
				.iter()
				.all(|order| order.account_id == INVENTORY_STRATEGY_ID));
			for strategy_id in [BASIC_STRATEGY_ID, SIMPLE_STRATEGY_ID] {
				assert_eq!(MockBalance::get_balance(&strategy_id, base_asset), 0);
				assert_eq!(MockBalance::get_balance(&strategy_id, STABLE_ASSET), 0);
			}
			assert_eq!(MockBalance::get_balance(&OWNER, base_asset), 2 * AMOUNT);
			assert_eq!(MockBalance::get_balance(&OWNER, STABLE_ASSET), 2 * AMOUNT);
		});
	}
}
