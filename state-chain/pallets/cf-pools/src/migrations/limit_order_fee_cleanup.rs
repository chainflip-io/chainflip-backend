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

use crate::Pools;
use cf_amm::limit_orders;
use frame_support::{traits::UncheckedOnRuntimeUpgrade, weights::Weight};
use sp_std::marker::PhantomData;

pub struct RemoveLimitOrderFeeFromPoolState<T>(PhantomData<T>);

mod old {

	use core::ops::Range;
	use frame_support::{pallet_prelude::OptionQuery, storage_alias, Twox64Concat};
	use sp_std::collections::btree_map::BTreeMap;

	use cf_primitives::Tick;
	use cf_traits::{OrderId, PoolPairsMap};
	use codec::{Decode, Encode};

	use crate::{AssetPair, Config, Pallet};

	use super::*;

	#[storage_alias(pallet_name)]
	pub type Pools<T: Config> =
		StorageMap<Pallet<T>, Twox64Concat, AssetPair, Pool<T>, OptionQuery>;

	#[derive(Decode, Encode)]
	pub struct PoolState<LiquidityProvider: Ord> {
		pub limit_orders: limit_orders::migration_support::PoolStateV7<LiquidityProvider>,
		pub range_orders: cf_amm::range_orders::PoolState<LiquidityProvider>,
	}

	#[derive(Decode, Encode)]
	pub struct Pool<T: Config> {
		/// A cache of all the range orders that exist in the pool. This must be kept up to date
		/// with the underlying pool.
		pub range_orders_cache: BTreeMap<T::AccountId, BTreeMap<OrderId, Range<Tick>>>,
		/// A cache of all the limit orders that exist in the pool. This must be kept up to date
		/// with the underlying pool. These are grouped by the asset the limit order is selling
		pub limit_orders_cache: PoolPairsMap<BTreeMap<T::AccountId, BTreeMap<OrderId, Tick>>>,
		pub pool_state: PoolState<(T::AccountId, OrderId)>,
	}
}

impl<T: crate::Config> UncheckedOnRuntimeUpgrade for RemoveLimitOrderFeeFromPoolState<T> {
	fn on_runtime_upgrade() -> Weight {
		use frame_support::traits::Get;

		let mut pools_migrated = 0u64;

		Pools::<T>::translate::<old::Pool<T>, _>(|asset_pair, old_pool| {
			log::info!(
				"Migrating pool for asset pair {:?}, removing fee_hundredth_pips from limit_orders PoolState",
				asset_pair
			);

			pools_migrated += 1;

			Some(crate::Pool {
				range_orders_cache: old_pool.range_orders_cache,
				limit_orders_cache: old_pool.limit_orders_cache,
				pool_state: cf_amm::PoolState {
					limit_orders: old_pool.pool_state.limit_orders.migrate_to_v8(),
					range_orders: old_pool.pool_state.range_orders,
				},
			})
		});

		log::info!("Migrated {} pools", pools_migrated);

		T::DbWeight::get().reads_writes(pools_migrated, pools_migrated)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<sp_std::vec::Vec<u8>, sp_runtime::DispatchError> {
		use codec::Encode;

		let pool_count = old::Pools::<T>::iter().count() as u32;
		log::info!("Pre-upgrade: Found {} pools to migrate", pool_count);

		Ok(pool_count.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: sp_std::vec::Vec<u8>) -> Result<(), sp_runtime::DispatchError> {
		use codec::Decode;

		let old_pool_count =
			u32::decode(&mut &state[..]).map_err(|_| "Failed to decode pool count")?;
		let new_pool_count = Pools::<T>::iter().count() as u32;

		assert_eq!(
			old_pool_count, new_pool_count,
			"Pool count should remain the same after migration"
		);

		log::info!("Post-upgrade: Successfully migrated {} pools", new_pool_count);

		Ok(())
	}
}
