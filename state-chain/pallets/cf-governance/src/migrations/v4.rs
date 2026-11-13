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

use crate::{Config, Council, Members};
use frame_support::{pallet_prelude::Weight, traits::UncheckedOnRuntimeUpgrade};
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use crate::council::MAX_MEMBERS;
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
		// `get` also returns `None` if the stored council fails to decode.
		let Some(old::GovernanceCouncil { members, threshold }) = old::Members::<T>::get() else {
			log::error!("No decodable governance council to migrate");
			return Weight::zero()
		};
		log::info!(
			"Migrating flat governance council {:?} with threshold {} to a nested council",
			members,
			threshold,
		);
		match Council::simple_group(u8::try_from(threshold).unwrap_or(u8::MAX), members) {
			Ok(council) => Members::<T>::put(council),
			// Leaves the old council in place, for a follow-up migration to recover.
			Err(error) => log::error!("Failed to migrate the governance council: {error:?}"),
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		// `get` also returns `None` if the stored council fails to decode.
		let council = old::Members::<T>::get()
			.ok_or(DispatchError::Other("No decodable governance council to migrate"))?;
		ensure!(
			council.members.len() <= MAX_MEMBERS,
			DispatchError::Other("The governance council has too many members to migrate")
		);
		Ok(council.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let old::GovernanceCouncil { members, threshold } =
			old::GovernanceCouncil::<T::AccountId>::decode(&mut &state[..])
				.map_err(|_| DispatchError::Other("Failed to decode the old governance council"))?;
		let council = Members::<T>::get();
		ensure!(
			Ok(&council) ==
				Council::simple_group(u8::try_from(threshold).unwrap_or(u8::MAX), members)
					.as_ref(),
			DispatchError::Other("Migrated council does not match the old one")
		);
		council
			.validate()
			.map_err(|_| DispatchError::Other("Migrated council is invalid"))?;
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

			assert_eq!(Ok(Members::<Test>::get()), Council::simple_group(2, [ALICE, BOB, CHARLES]));
		});
	}

	#[test]
	fn absent_council_reads_as_default() {
		new_test_ext().execute_with(|| {
			Members::<Test>::kill();

			Migration::<Test>::on_runtime_upgrade();

			assert!(!Members::<Test>::exists());
			assert_eq!(Members::<Test>::get(), Council::default());
		});
	}
}
