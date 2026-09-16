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
//! operator relation per delegator. To support multi-operator delegation, its value is reshaped
//! in place into `DelegationChoice: StorageMap<delegator, DelegationPlan>`, where `DelegationPlan`
//! holds a full `operator -> max_bid` set instead of a single pair. This migration translates each
//! pre-existing single-relation entry into the equivalent one-entry plan, preserving all
//! delegator/operator/max_bid data exactly, and in the same pass backfills the new
//! `OperatorDelegators` reverse index (operator -> delegator) that lets "who delegates to this
//! operator" be answered without scanning every delegator in the system.

use crate::{Config, DelegationChoice, DelegationPlan, OperatorDelegators};
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
			DelegationChoice::<T>::insert(
				&delegator,
				DelegationPlan::try_from_map(BTreeMap::from([(operator.clone(), max_bid)]))
					.unwrap_or_else(|_| {
						cf_runtime_utilities::log_or_panic!(
							"migrated delegator relation exceeded MaxOperatorsPerDelegator"
						);
						Default::default()
					}),
			);
			OperatorDelegators::<T>::insert(operator, &delegator, ());
			entries_migrated.saturating_accrue(1);
		}

		T::DbWeight::get()
			.reads_writes(entries_migrated.saturating_add(1), entries_migrated.saturating_mul(3))
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
		let entries_count = entries.len();

		for (delegator, operator, max_bid) in &entries {
			let plan = DelegationChoice::<T>::get(delegator)
				.ok_or(DispatchError::Other("expected migrated DelegationChoice entry"))?;
			frame_support::ensure!(
				plan.into_map().get(operator) == Some(max_bid),
				DispatchError::Other("migrated max_bid did not match its pre-upgrade value")
			);
			frame_support::ensure!(
				OperatorDelegators::<T>::contains_key(operator, delegator),
				DispatchError::Other("expected backfilled OperatorDelegators entry")
			);
		}
		frame_support::ensure!(
			DelegationChoice::<T>::iter().count() == entries_count,
			DispatchError::Other("migrated entry count did not match pre-upgrade entry count")
		);
		frame_support::ensure!(
			OperatorDelegators::<T>::iter().count() == entries_count,
			DispatchError::Other(
				"OperatorDelegators entry count did not match pre-upgrade entry count"
			)
		);
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, MockFlip, Test, ALICE, BOB};

	const OTHER_DELEGATOR: u64 = 102;
	const OTHER_OPERATOR: u64 = 103;

	#[test]
	fn migrates_single_relation_entries_and_backfills_index() {
		new_test_ext().execute_with(|| {
			MockFlip::credit_funds(&ALICE, 1_000);
			MockFlip::credit_funds(&OTHER_DELEGATOR, 500);
			old::DelegationChoice::<Test>::insert(ALICE, (BOB, 1_000));
			old::DelegationChoice::<Test>::insert(OTHER_DELEGATOR, (OTHER_OPERATOR, 500));

			#[cfg(feature = "try-runtime")]
			let state = Migration::<Test>::pre_upgrade().unwrap();

			Migration::<Test>::on_runtime_upgrade();

			#[cfg(feature = "try-runtime")]
			Migration::<Test>::post_upgrade(state).unwrap();

			assert_eq!(
				DelegationChoice::<Test>::get(ALICE).unwrap().into_map(),
				BTreeMap::from([(BOB, 1_000)])
			);
			assert_eq!(
				DelegationChoice::<Test>::get(OTHER_DELEGATOR).unwrap().into_map(),
				BTreeMap::from([(OTHER_OPERATOR, 500)])
			);
			assert_eq!(DelegationChoice::<Test>::iter().count(), 2);

			assert!(OperatorDelegators::<Test>::contains_key(BOB, ALICE));
			assert!(OperatorDelegators::<Test>::contains_key(OTHER_OPERATOR, OTHER_DELEGATOR));
			assert_eq!(OperatorDelegators::<Test>::iter().count(), 2);
		});
	}

	#[test]
	fn no_entries_is_a_noop() {
		new_test_ext().execute_with(|| {
			Migration::<Test>::on_runtime_upgrade();
			assert!(DelegationChoice::<Test>::iter().next().is_none());
			assert!(OperatorDelegators::<Test>::iter().next().is_none());
		});
	}
}
