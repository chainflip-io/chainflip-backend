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

//! Range orders are now subject to the same per-asset minimum as limit orders, so the single
//! `MinimumOrderAmount` map replaces `MinimumLimitOrderAmount`. Values carry over untouched; only
//! the storage name changes.

use crate::{Config, MinimumOrderAmount};
use frame_support::{
	pallet_prelude::Weight,
	sp_runtime::Saturating,
	traits::{Get, UncheckedOnRuntimeUpgrade},
};
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use cf_primitives::{Asset, AssetAmount};
#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
#[cfg(feature = "try-runtime")]
use frame_support::{ensure, pallet_prelude::DispatchError};
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

mod old {
	use crate::{Config, Pallet};
	use cf_primitives::{Asset, AssetAmount};
	use frame_support::{pallet_prelude::ValueQuery, Twox64Concat};

	#[frame_support::storage_alias]
	pub(super) type MinimumLimitOrderAmount<T: Config> =
		StorageMap<Pallet<T>, Twox64Concat, Asset, AssetAmount, ValueQuery>;
}

pub struct Migration<T>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		let mut entries: u64 = 0;

		for (asset, amount) in old::MinimumLimitOrderAmount::<T>::drain() {
			entries.saturating_accrue(1);
			MinimumOrderAmount::<T>::insert(asset, amount);
		}

		// One read and two writes (clearing the old key, writing the new one) per entry.
		T::DbWeight::get().reads_writes(entries, entries.saturating_mul(2))
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		ensure!(
			MinimumOrderAmount::<T>::iter().next().is_none(),
			"MinimumOrderAmount should not exist before the migration"
		);
		Ok(old::MinimumLimitOrderAmount::<T>::iter()
			.collect::<Vec<(Asset, AssetAmount)>>()
			.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let old_entries = <Vec<(Asset, AssetAmount)>>::decode(&mut &state[..])
			.map_err(|_| "failed to decode pre_upgrade state")?;

		ensure!(
			old::MinimumLimitOrderAmount::<T>::iter().next().is_none(),
			"MinimumLimitOrderAmount should have been drained"
		);
		ensure!(
			MinimumOrderAmount::<T>::iter().count() == old_entries.len(),
			"MinimumOrderAmount entry count does not match the old storage"
		);
		for (asset, amount) in old_entries {
			ensure!(
				MinimumOrderAmount::<T>::get(asset) == amount,
				"MinimumOrderAmount value does not match the old storage"
			);
		}

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, Test};
	use cf_primitives::Asset;

	#[test]
	fn migration_moves_values_to_the_renamed_storage() {
		new_test_ext().execute_with(|| {
			old::MinimumLimitOrderAmount::<Test>::insert(Asset::Eth, 1_000);
			old::MinimumLimitOrderAmount::<Test>::insert(Asset::Usdc, 2_000);

			#[cfg(feature = "try-runtime")]
			let state = Migration::<Test>::pre_upgrade().unwrap();
			Migration::<Test>::on_runtime_upgrade();
			#[cfg(feature = "try-runtime")]
			Migration::<Test>::post_upgrade(state).unwrap();

			assert_eq!(MinimumOrderAmount::<Test>::get(Asset::Eth), 1_000);
			assert_eq!(MinimumOrderAmount::<Test>::get(Asset::Usdc), 2_000);
			// Unset assets keep the ValueQuery default, as before.
			assert_eq!(MinimumOrderAmount::<Test>::get(Asset::Btc), 0);
			assert!(old::MinimumLimitOrderAmount::<Test>::iter().next().is_none());
		});
	}
}
