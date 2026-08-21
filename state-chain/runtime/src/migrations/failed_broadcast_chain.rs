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

//! Migrates the legacy global failed-broadcast penalty and removes its suspensions.

use crate::{runtime_apis::types::before_version_21, DbWeight, Runtime};
use cf_primitives::ForeignChain;
use frame_support::{
	migrations::VersionedMigration, traits::UncheckedOnRuntimeUpgrade, weights::Weight,
};

#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

mod old {
	use super::*;
	use frame_support::{pallet_prelude::OptionQuery, storage_alias, Twox64Concat};

	#[storage_alias]
	pub type Penalties = StorageMap<
		Reputation,
		Twox64Concat,
		before_version_21::Offence,
		pallet_cf_reputation::Penalty<Runtime>,
		OptionQuery,
	>;

	#[storage_alias]
	pub type Suspensions = StorageMap<
		Reputation,
		Twox64Concat,
		before_version_21::Offence,
		sp_std::collections::vec_deque::VecDeque<(crate::BlockNumber, crate::AccountId)>,
		OptionQuery,
	>;
}

const OLD_STORAGE_VERSION: u16 = 0;

pub type Migration = VersionedMigration<
	OLD_STORAGE_VERSION,
	{ pallet_cf_reputation::STORAGE_VERSION_U16 },
	Migrate,
	pallet_cf_reputation::Pallet<Runtime>,
	DbWeight,
>;

pub struct Migrate;

impl UncheckedOnRuntimeUpgrade for Migrate {
	fn on_runtime_upgrade() -> Weight {
		let legacy_offence = before_version_21::Offence::FailedToBroadcastTransaction;
		let mut writes = 2u64;

		if let Some(penalty) = old::Penalties::take(legacy_offence) {
			// Genesis configuration is not reapplied to live state during an upgrade.
			for chain in ForeignChain::iter() {
				pallet_cf_reputation::Penalties::<Runtime>::insert(
					crate::chainflip::Offence::FailedToBroadcastTransaction(chain),
					penalty.clone(),
				);
				writes = writes.saturating_add(1);
			}
		}

		old::Suspensions::remove(legacy_offence);

		<Runtime as frame_system::Config>::DbWeight::get().reads_writes(1, writes)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		Ok(old::Penalties::get(before_version_21::Offence::FailedToBroadcastTransaction).encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
		let legacy_penalty: Option<pallet_cf_reputation::Penalty<Runtime>> =
			Decode::decode(&mut &state[..]).map_err(|_| {
				TryRuntimeError::Other("failed to decode failed-broadcast migration state")
			})?;

		frame_support::ensure!(
			old::Penalties::get(before_version_21::Offence::FailedToBroadcastTransaction).is_none(),
			"legacy failed-broadcast penalty was not removed",
		);
		frame_support::ensure!(
			old::Suspensions::get(before_version_21::Offence::FailedToBroadcastTransaction)
				.is_none(),
			"legacy failed-broadcast suspensions were not removed",
		);

		for chain in ForeignChain::iter() {
			let offence = crate::chainflip::Offence::FailedToBroadcastTransaction(chain);
			if let Some(ref penalty) = legacy_penalty {
				frame_support::ensure!(
					pallet_cf_reputation::Penalties::<Runtime>::get(offence) == *penalty,
					"failed-broadcast penalty was not migrated",
				);
			}
		}

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use frame_support::traits::{GetStorageVersion, OnRuntimeUpgrade, StorageVersion};
	use pallet_cf_reputation::Penalty;
	use sp_runtime::AccountId32;
	use sp_std::collections::vec_deque::VecDeque;

	#[test]
	fn migrates_legacy_penalty_and_discards_suspensions() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			assert_eq!(
				pallet_cf_reputation::Pallet::<Runtime>::in_code_storage_version(),
				StorageVersion::new(pallet_cf_reputation::STORAGE_VERSION_U16),
			);
			assert_eq!(
				pallet_cf_reputation::Pallet::<Runtime>::on_chain_storage_version(),
				StorageVersion::new(OLD_STORAGE_VERSION),
			);

			let legacy_offence = before_version_21::Offence::FailedToBroadcastTransaction;
			let penalty = Penalty::<Runtime> { reputation: 10, suspension: 20 };
			old::Penalties::insert(legacy_offence, penalty.clone());
			old::Suspensions::insert(
				legacy_offence,
				VecDeque::from([(30, AccountId32::new([1; 32]))]),
			);

			Migration::on_runtime_upgrade();
			assert_eq!(
				pallet_cf_reputation::Pallet::<Runtime>::on_chain_storage_version(),
				StorageVersion::new(pallet_cf_reputation::STORAGE_VERSION_U16),
			);

			assert!(old::Penalties::get(legacy_offence).is_none());
			assert!(old::Suspensions::get(legacy_offence).is_none());
			for chain in ForeignChain::iter() {
				let offence = crate::chainflip::Offence::FailedToBroadcastTransaction(chain);
				assert_eq!(pallet_cf_reputation::Penalties::<Runtime>::get(offence), penalty,);
				assert!(!pallet_cf_reputation::Suspensions::<Runtime>::contains_key(offence));
			}

			// The version guard makes subsequent executions a no-op.
			Migration::on_runtime_upgrade();
			for chain in ForeignChain::iter() {
				let offence = crate::chainflip::Offence::FailedToBroadcastTransaction(chain);
				assert_eq!(pallet_cf_reputation::Penalties::<Runtime>::get(offence), penalty,);
				assert!(!pallet_cf_reputation::Suspensions::<Runtime>::contains_key(offence));
			}
		});
	}
}
