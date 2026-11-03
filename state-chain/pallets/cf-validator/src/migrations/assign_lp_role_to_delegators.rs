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

//! `delegate` used to be available to any account that wasn't a Validator or Operator, and only
//! started requiring (and auto-assigning) the Liquidity Provider role once that was enforced.
//! Pre-existing delegators therefore need the role backfilled here so they satisfy the same
//! invariant going forward.

use crate::Config;
use cf_primitives::AccountRole;
use cf_traits::AccountRoleRegistry;
use frame_support::{
	pallet_prelude::Weight,
	sp_runtime::Saturating,
	traits::{Get, UncheckedOnRuntimeUpgrade},
};
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

use super::unify_delegation_choice::old;

pub struct Migration<T>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		let mut reads: u64 = 0;
		let mut writes: u64 = 0;

		for delegator in old::DelegationChoice::<T>::iter_keys() {
			reads.saturating_accrue(1);
			match T::AccountRoleRegistry::account_role(&delegator) {
				AccountRole::Unregistered => {
					if T::AccountRoleRegistry::register_as_liquidity_provider(&delegator).is_ok() {
						writes.saturating_accrue(1);
					} else {
						// should be unreachable
						cf_runtime_utilities::log_or_panic!(
							"Failed to register pre-existing delegator {:?} as a Liquidity Provider",
							delegator
						);
					}
				},
				AccountRole::LiquidityProvider => {},
				// it was technically possible to delegate with a broker account role and an edge
				// case where you could have operator or validator role after delegating. Its not
				// immediately clear if we should convert it to LP here. We should log it at least,
				// to surface these cases
				other => log::warn!(
					"Delegator {:?} has incompatible role {:?}; expected Unregistered or LiquidityProvider",
					delegator,
					other
				),
			}
		}

		T::DbWeight::get().reads_writes(reads, writes)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let delegators_missing_lp_role: Vec<T::AccountId> = old::DelegationChoice::<T>::iter_keys()
			.filter(|delegator| {
				!T::AccountRoleRegistry::has_account_role(delegator, AccountRole::LiquidityProvider)
			})
			.collect();
		Ok(delegators_missing_lp_role.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let delegators_missing_lp_role: Vec<T::AccountId> =
			Decode::decode(&mut &state[..]).map_err(|_| "failed to decode pre_upgrade state")?;

		for delegator in delegators_missing_lp_role {
			frame_support::ensure!(
				T::AccountRoleRegistry::has_account_role(
					&delegator,
					AccountRole::LiquidityProvider
				),
				"delegator should have been assigned the Liquidity Provider role"
			);
		}

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, MockFlip, Test, ALICE, BOB};

	fn is_liquidity_provider(who: &u64) -> bool {
		<<Test as cf_traits::Chainflip>::AccountRoleRegistry as AccountRoleRegistry<Test>>::has_account_role(
			who,
			AccountRole::LiquidityProvider,
		)
	}

	#[test]
	fn backfills_lp_role_for_unregistered_delegator() {
		new_test_ext().execute_with(|| {
			MockFlip::credit_funds(&ALICE, 1_000);
			old::DelegationChoice::<Test>::insert(ALICE, (BOB, 1_000));

			assert!(!is_liquidity_provider(&ALICE));

			#[cfg(feature = "try-runtime")]
			let state = Migration::<Test>::pre_upgrade().unwrap();

			Migration::<Test>::on_runtime_upgrade();

			#[cfg(feature = "try-runtime")]
			Migration::<Test>::post_upgrade(state).unwrap();

			assert!(is_liquidity_provider(&ALICE));
		});
	}

	#[test]
	fn non_delegators_are_left_alone() {
		new_test_ext().execute_with(|| {
			MockFlip::credit_funds(&ALICE, 1_000);

			Migration::<Test>::on_runtime_upgrade();

			assert!(!is_liquidity_provider(&ALICE));
		});
	}
}
