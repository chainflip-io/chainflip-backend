// Copyright 2026 Chainflip Labs GmbH
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

use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};

#[cfg(feature = "try-runtime")]
use frame_support::ensure;
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

/// The storage items the emissions pallet had at the time it was removed from
/// `construct_runtime!`. `Emissions` is the pallet's `construct_runtime!` name, i.e. its storage
/// prefix - it doesn't need to resolve to an actual pallet for `#[storage_alias]` to use it as
/// one, which lets these aliases outlive the pallet's removal.
#[cfg(test)]
mod old {
	use crate::Runtime;
	use frame_support::{pallet_prelude::ValueQuery, storage_alias};
	use frame_system::pallet_prelude::BlockNumberFor;

	#[storage_alias]
	pub type LastSupplyUpdateBlock = StorageValue<Emissions, BlockNumberFor<Runtime>, ValueQuery>;

	#[storage_alias]
	pub type CurrentAuthorityEmissionPerBlock = StorageValue<Emissions, crate::Balance, ValueQuery>;

	#[storage_alias]
	pub type CurrentAuthorityEmissionInflation = StorageValue<Emissions, u32, ValueQuery>;

	#[storage_alias]
	pub type SupplyUpdateInterval = StorageValue<Emissions, BlockNumberFor<Runtime>, ValueQuery>;
}

/// The emissions pallet was removed from `construct_runtime!` now that FLIP 2.1 is
/// unconditionally active. Removing a pallet doesn't clear its storage, so this clears the
/// orphaned `Emissions` prefix directly. The pallet only ever had a handful of `StorageValue`
/// items, so a single unbounded `clear_prefix` is safe here.
pub struct RemoveEmissionsStorage;

impl OnRuntimeUpgrade for RemoveEmissionsStorage {
	fn on_runtime_upgrade() -> Weight {
		let _ = frame_support::storage::unhashed::clear_prefix(
			&sp_core::hashing::twox_128(b"Emissions"),
			None,
			None,
		);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		use codec::Encode;
		let had_emissions_storage = frame_support::storage::unhashed::contains_prefixed_key(
			&sp_core::hashing::twox_128(b"Emissions"),
		);
		Ok(had_emissions_storage.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
		ensure!(
			!frame_support::storage::unhashed::contains_prefixed_key(&sp_core::hashing::twox_128(
				b"Emissions"
			)),
			"Emissions storage prefix not cleared"
		);
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	// Populates the storage via the pallet's own typed storage items (the `old` aliases), so this
	// proves the raw `twox_128(b"Emissions")` prefix used by the migration actually maps onto the
	// real storage the emissions pallet wrote, not just onto a key we made up ourselves.
	#[test]
	fn clears_the_actual_emissions_pallet_storage() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			old::LastSupplyUpdateBlock::put(100);
			old::CurrentAuthorityEmissionPerBlock::put(42u128);
			old::CurrentAuthorityEmissionInflation::put(7);
			old::SupplyUpdateInterval::put(50);

			RemoveEmissionsStorage::on_runtime_upgrade();

			assert!(!old::LastSupplyUpdateBlock::exists());
			assert!(!old::CurrentAuthorityEmissionPerBlock::exists());
			assert!(!old::CurrentAuthorityEmissionInflation::exists());
			assert!(!old::SupplyUpdateInterval::exists());
		});
	}

	#[cfg(feature = "try-runtime")]
	#[test]
	fn pre_and_post_upgrade_round_trip() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			old::CurrentAuthorityEmissionPerBlock::put(42u128);

			let state = RemoveEmissionsStorage::pre_upgrade().unwrap();

			RemoveEmissionsStorage::on_runtime_upgrade();

			assert!(RemoveEmissionsStorage::post_upgrade(state).is_ok());
		});
	}
}
