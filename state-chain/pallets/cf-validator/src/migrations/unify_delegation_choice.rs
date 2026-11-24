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

//! Reshapes `DelegationChoice` from a single `(operator, max_bid)` per delegator into a
//! `DelegationPlan`. Each entry becomes `{operator: 100%}` with
//! `AmountWithoutCompounding(max_bid)`, which resolves to the same bid as before. One
//! `DelegationPlanUpdated` event (reason `Migrated`) is emitted per delegator so that indexers get
//! a starting plan to compare later changes against.

#[cfg(any(test, feature = "try-runtime"))]
use crate::DelegationChoice;
use crate::{Config, DelegationChangeReason, DelegationPlan, MaxBid, Pallet};
use frame_support::{
	pallet_prelude::Weight,
	sp_runtime::Saturating,
	traits::{Get, UncheckedOnRuntimeUpgrade},
};
use sp_std::{marker::PhantomData, vec::Vec};

#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::DispatchError;

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

		// Collected first: the old and new items share a storage key.
		let old_entries: Vec<_> = old::DelegationChoice::<T>::drain().collect();
		for (delegator, (operator, max_bid)) in old_entries {
			Pallet::<T>::set_delegation_plan(
				&delegator,
				Some(DelegationPlan::single(operator, MaxBid::AmountWithoutCompounding(max_bid))),
				DelegationChangeReason::Migrated,
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
		let entries_count = entries.len();

		for (delegator, operator, max_bid) in entries {
			let plan = DelegationChoice::<T>::get(&delegator)
				.ok_or(DispatchError::Other("expected migrated DelegationChoice entry"))?;
			frame_support::ensure!(
				plan == DelegationPlan::single(operator, MaxBid::AmountWithoutCompounding(max_bid)),
				DispatchError::Other("migrated plan did not match its pre-upgrade value")
			);
		}
		frame_support::ensure!(
			DelegationChoice::<T>::iter().count() == entries_count,
			DispatchError::Other("migrated entry count did not match pre-upgrade entry count")
		);
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, MockFlip, RuntimeEvent, System, Test, ALICE, BOB};

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
				DelegationChoice::<Test>::get(ALICE).unwrap(),
				DelegationPlan::single(BOB, MaxBid::AmountWithoutCompounding(1_000))
			);
			assert_eq!(
				DelegationChoice::<Test>::get(OTHER_DELEGATOR).unwrap(),
				DelegationPlan::single(BOB, MaxBid::AmountWithoutCompounding(500))
			);
			assert_eq!(DelegationChoice::<Test>::iter().count(), 2);

			// Indexers get each delegator's starting plan.
			System::assert_has_event(RuntimeEvent::ValidatorPallet(
				crate::Event::DelegationPlanUpdated {
					delegator: ALICE,
					old_plan: None,
					new_plan: Some(
						DelegationPlan::<_, _, crate::DelegationPlanLimitsOf<Test>>::single(
							BOB,
							MaxBid::AmountWithoutCompounding(1_000),
						)
						.details(1_000),
					),
					reason: DelegationChangeReason::Migrated,
				},
			));
		});
	}

	#[test]
	fn no_entries_is_a_noop() {
		new_test_ext().execute_with(|| {
			Migration::<Test>::on_runtime_upgrade();
			assert!(DelegationChoice::<Test>::iter().next().is_none());
		});
	}
}
