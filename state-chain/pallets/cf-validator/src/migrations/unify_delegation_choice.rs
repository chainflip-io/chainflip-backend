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

//! `DelegationChoice: StorageMap<delegator, (operator, max_bid)>` only ever supported a single
//! operator relation per delegator. To support multi-operator delegation, it's reshaped into
//! `DelegationChoices: StorageMap<delegator, DelegatorRelations>`, where `DelegatorRelations`
//! holds a full `operator -> max_bid` map instead of a single pair. This migration translates
//! each pre-existing single-relation entry into the equivalent one-entry map, preserving all
//! delegator/operator/max_bid data exactly.

use crate::{Config, DelegationChoices, DelegatorRelations};
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

/// Shared with `assign_lp_role_to_delegators` -- both migrations read/write the same
/// pre-version-11 `DelegationChoice` storage item.
pub(super) mod old {
	use super::*;
	use frame_support::{pallet_prelude::OptionQuery, storage_alias, Identity};

	#[storage_alias]
	pub type DelegationChoice<T: Config> = StorageMap<
		crate::Pallet<T>,
		Identity,
		<T as frame_system::Config>::AccountId,
		(<T as frame_system::Config>::AccountId, <T as cf_traits::Chainflip>::Amount),
		OptionQuery,
	>;
}

pub struct Migration<T>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		let mut entries_migrated: u64 = 0;

		for (delegator, (operator, max_bid)) in old::DelegationChoice::<T>::drain() {
			DelegationChoices::<T>::insert(
				&delegator,
				DelegatorRelations { operators: BTreeMap::from([(operator, max_bid)]) },
			);
			entries_migrated.saturating_accrue(1);
		}

		T::DbWeight::get()
			.reads_writes(entries_migrated.saturating_add(1), entries_migrated.saturating_mul(2))
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let entries: Vec<(T::AccountId, T::AccountId, T::Amount)> =
			old::DelegationChoice::<T>::iter()
				.map(|(delegator, (operator, max_bid))| (delegator, operator, max_bid))
				.collect();
		Ok(entries.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let entries: Vec<(T::AccountId, T::AccountId, T::Amount)> =
			Decode::decode(&mut &state[..]).map_err(|_| "failed to decode pre_upgrade state")?;

		for (delegator, operator, max_bid) in entries {
			let relations = DelegationChoices::<T>::get(&delegator)
				.ok_or(DispatchError::Other("expected migrated DelegationChoices entry"))?;
			frame_support::ensure!(
				relations.operators.get(&operator) == Some(&max_bid),
				DispatchError::Other("migrated max_bid did not match its pre-upgrade value")
			);
		}
		frame_support::ensure!(
			old::DelegationChoice::<T>::iter().next().is_none(),
			DispatchError::Other("old DelegationChoice storage was not fully drained")
		);
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, MockFlip, Test, ALICE, BOB};

	const OTHER_DELEGATOR: u64 = 102;

	#[test]
	fn migrates_single_relation_entries() {
		new_test_ext().execute_with(|| {
			MockFlip::credit_funds(&ALICE, 1_000);
			MockFlip::credit_funds(&OTHER_DELEGATOR, 500);
			old::DelegationChoice::<Test>::insert(ALICE, (BOB, 1_000));
			old::DelegationChoice::<Test>::insert(OTHER_DELEGATOR, (BOB, 500));

			#[cfg(feature = "try-runtime")]
			let state = Migration::<Test>::pre_upgrade().unwrap();

			Migration::<Test>::on_runtime_upgrade();

			#[cfg(feature = "try-runtime")]
			Migration::<Test>::post_upgrade(state).unwrap();

			assert_eq!(
				DelegationChoices::<Test>::get(ALICE).unwrap().operators,
				BTreeMap::from([(BOB, 1_000)])
			);
			assert_eq!(
				DelegationChoices::<Test>::get(OTHER_DELEGATOR).unwrap().operators,
				BTreeMap::from([(BOB, 500)])
			);
			assert!(old::DelegationChoice::<Test>::iter().next().is_none());
		});
	}

	#[test]
	fn no_entries_is_a_noop() {
		new_test_ext().execute_with(|| {
			Migration::<Test>::on_runtime_upgrade();
			assert!(DelegationChoices::<Test>::iter().next().is_none());
		});
	}
}
