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

use crate::Config;
use frame_support::{
	traits::{Get, UncheckedOnRuntimeUpgrade},
	weights::Weight,
};
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

mod old {
	use crate::{Config, Pallet};
	use frame_support::{storage_alias, Twox64Concat};
	use frame_system::pallet_prelude::BlockNumberFor;
	use sp_std::vec::Vec;

	#[storage_alias]
	pub type BoostDelayBlocks<T: Config<I>, I: 'static> =
		StorageValue<Pallet<T, I>, BlockNumberFor<T>>;

	// The values are never decoded, so their type is irrelevant.
	#[storage_alias]
	pub type PendingPrewitnessedDeposits<T: Config<I>, I: 'static> =
		StorageMap<Pallet<T, I>, Twox64Concat, BlockNumberFor<T>, Vec<u8>>;
}

pub struct Migration<T, I>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> Weight {
		old::BoostDelayBlocks::<T, I>::kill();
		let removed = old::PendingPrewitnessedDeposits::<T, I>::clear(u32::MAX, None).unique;
		T::DbWeight::get().writes(1u64.saturating_add(removed as u64))
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		// `BoostStatus::BoostPending` was removed, so any pending entry would leave behind a
		// boost status that can no longer be decoded.
		frame_support::ensure!(
			old::PendingPrewitnessedDeposits::<T, I>::iter_keys().next().is_none(),
			"PendingPrewitnessedDeposits should be empty"
		);
		Ok(Vec::new())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		frame_support::ensure!(
			!old::BoostDelayBlocks::<T, I>::exists(),
			"BoostDelayBlocks storage should have been removed"
		);
		frame_support::ensure!(
			old::PendingPrewitnessedDeposits::<T, I>::iter_keys().next().is_none(),
			"PendingPrewitnessedDeposits storage should have been removed"
		);
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mocks::{new_test_ext, Test};
	use frame_support::instances::Instance1;

	#[test]
	fn removes_boost_delay_storage() {
		new_test_ext().execute_with(|| {
			old::BoostDelayBlocks::<Test, Instance1>::put(10);
			old::PendingPrewitnessedDeposits::<Test, Instance1>::insert(5, vec![1, 2, 3]);

			Migration::<Test, Instance1>::on_runtime_upgrade();

			assert!(!old::BoostDelayBlocks::<Test, Instance1>::exists());
			assert!(old::PendingPrewitnessedDeposits::<Test, Instance1>::iter_keys()
				.next()
				.is_none());
		});
	}
}
