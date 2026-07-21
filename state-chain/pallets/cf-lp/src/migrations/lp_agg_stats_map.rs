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

use crate::{AggStats, Config};
use frame_support::{
	pallet_prelude::Weight,
	sp_runtime::Saturating,
	traits::{Get, UncheckedOnRuntimeUpgrade},
};
use sp_std::{collections::btree_map::BTreeMap, marker::PhantomData};

#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

mod old {
	use super::*;
	use cf_primitives::Asset;
	use frame_support::{pallet_prelude::ValueQuery, storage_alias};

	#[storage_alias]
	pub type LpAggStats<T: crate::Config> = StorageValue<
		crate::Pallet<T>,
		BTreeMap<<T as frame_system::Config>::AccountId, BTreeMap<Asset, AggStats>>,
		ValueQuery,
	>;
}

pub struct Migration<T>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		let old_map = old::LpAggStats::<T>::take();
		let mut entries_migrated: u64 = 0;

		for (lp, per_asset) in old_map {
			for (asset, agg_stats) in per_asset {
				crate::LpAggStats::<T>::insert(&lp, asset, agg_stats);
				entries_migrated.saturating_accrue(1);
			}
		}

		T::DbWeight::get().reads_writes(1, entries_migrated.saturating_add(1))
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let old_map = old::LpAggStats::<T>::get();
		let entry_count: u64 = old_map.values().map(|per_asset| per_asset.len() as u64).sum();
		Ok(entry_count.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let expected_count = u64::decode(&mut &state[..])
			.map_err(|_| DispatchError::Other("failed to decode pre_upgrade state"))?;
		let actual_count = crate::LpAggStats::<T>::iter().count() as u64;
		frame_support::ensure!(
			actual_count == expected_count,
			DispatchError::Other("LpAggStats entry count changed across migration")
		);
		frame_support::ensure!(
			old::LpAggStats::<T>::get().is_empty(),
			DispatchError::Other("old LpAggStats StorageValue was not cleared")
		);
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, Test, LP_ACCOUNT, LP_ACCOUNT_2};
	use cf_primitives::Asset;
	use sp_runtime::FixedU128;

	#[test]
	fn migrates_all_entries_and_clears_old_storage() {
		new_test_ext().execute_with(|| {
			let stats_1 = AggStats::new(crate::DeltaStats {
				limit_orders_swap_usd_volume: FixedU128::from_u32(100),
			});
			let stats_2 = AggStats::new(crate::DeltaStats {
				limit_orders_swap_usd_volume: FixedU128::from_u32(200),
			});

			let mut old_map = BTreeMap::new();
			old_map.insert(LP_ACCOUNT, BTreeMap::from([(Asset::Eth, stats_1)]));
			old_map.insert(
				LP_ACCOUNT_2,
				BTreeMap::from([(Asset::Flip, stats_2), (Asset::Usdc, stats_1)]),
			);
			old::LpAggStats::<Test>::put(old_map);

			#[cfg(feature = "try-runtime")]
			let state = Migration::<Test>::pre_upgrade().unwrap();

			Migration::<Test>::on_runtime_upgrade();

			#[cfg(feature = "try-runtime")]
			Migration::<Test>::post_upgrade(state).unwrap();

			assert_eq!(crate::LpAggStats::<Test>::get(LP_ACCOUNT, Asset::Eth), Some(stats_1));
			assert_eq!(crate::LpAggStats::<Test>::get(LP_ACCOUNT_2, Asset::Flip), Some(stats_2));
			assert_eq!(crate::LpAggStats::<Test>::get(LP_ACCOUNT_2, Asset::Usdc), Some(stats_1));
			assert_eq!(crate::LpAggStats::<Test>::iter().count(), 3);
			assert!(old::LpAggStats::<Test>::get().is_empty());
		});
	}
}
