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

use crate::{Config, Members, VotingAuthority};
use frame_support::{pallet_prelude::Weight, traits::UncheckedOnRuntimeUpgrade};
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
#[cfg(feature = "try-runtime")]
use frame_support::{ensure, pallet_prelude::DispatchError};
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

mod old {
	use crate::{Config, Pallet};
	use codec::{Decode, Encode};
	use frame_support::pallet_prelude::OptionQuery;
	use sp_std::collections::btree_set::BTreeSet;

	#[derive(Encode, Decode)]
	pub struct GovernanceCouncil<AccountId> {
		pub members: BTreeSet<AccountId>,
		#[codec(compact)]
		pub threshold: u32,
	}

	#[frame_support::storage_alias]
	pub type Members<T: Config> = StorageValue<
		Pallet<T>,
		GovernanceCouncil<<T as frame_system::Config>::AccountId>,
		OptionQuery,
	>;
}

pub struct Migration<T>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		let _ = Members::<T>::translate::<old::GovernanceCouncil<T::AccountId>, _>(|old| {
			old.map(|old::GovernanceCouncil { members, threshold }| {
				log::info!(
					"Migrating governance council {:?} with threshold {} to a voting authority",
					members,
					threshold,
				);
				VotingAuthority::simple_group(u8::try_from(threshold).unwrap_or(u8::MAX), members)
			})
		})
		.inspect_err(|_| log::error!("Failed to migrate the governance council"));

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(old::Members::<T>::get().encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		if let Some(old::GovernanceCouncil { members, threshold }) =
			Option::<old::GovernanceCouncil<T::AccountId>>::decode(&mut &state[..])
				.map_err(|_| DispatchError::Other("Failed to decode the old governance council"))?
		{
			let authority = Members::<T>::get();
			ensure!(
				authority ==
					VotingAuthority::simple_group(
						u8::try_from(threshold).unwrap_or(u8::MAX),
						members
					),
				DispatchError::Other("Voting authority does not match the old council")
			);
			authority
				.validate()
				.map_err(|_| DispatchError::Other("Migrated voting authority is invalid"))?;
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::*;
	use sp_std::collections::btree_set::BTreeSet;

	#[test]
	fn migrates_council_to_simple_group() {
		new_test_ext().execute_with(|| {
			old::Members::<Test>::put(old::GovernanceCouncil {
				members: BTreeSet::from([ALICE, BOB, CHARLES]),
				threshold: 2,
			});

			Migration::<Test>::on_runtime_upgrade();

			assert_eq!(
				Members::<Test>::get(),
				VotingAuthority::simple_group(2, [ALICE, BOB, CHARLES])
			);
		});
	}
}
