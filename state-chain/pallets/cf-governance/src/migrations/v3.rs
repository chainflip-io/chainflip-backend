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

use crate::*;
use frame_support::{pallet_prelude::Weight, traits::UncheckedOnRuntimeUpgrade};
use sp_std::{collections::btree_set::BTreeSet, marker::PhantomData};

#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

pub struct FixThresholdCalculation<T>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for FixThresholdCalculation<T> {
	fn on_runtime_upgrade() -> Weight {
		let _ = crate::Members::<T>::translate::<BTreeSet<T::AccountId>, _>(|old| {
			old.map(|members| {
				// We want to change the threshold to 3 on mainnet (previously 4 of 6).
				let threshold = (members.len() as u32).div_ceil(2);
				log::info!(
					"Upgrading governance council with members {:?}, setting threshold to {}",
					members,
					threshold,
				);
				GovernanceCouncil { members, threshold }
			})
		})
		.inspect_err(|_| {
			log::error!("Failed to migrate governance council members");
		});

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let old_council = frame_support::storage::unhashed::get::<BTreeSet<T::AccountId>>(
			&Members::<T>::hashed_key()[..],
		);
		Ok(old_council.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let old_council = BTreeSet::<T::AccountId>::decode(&mut &state[..])
			.map_err(|_| DispatchError::Other("Failed to decode member count"))?;
		let council = Members::<T>::get();
		let expected_threshold = (council.members.len() as u32).div_ceil(2);
		assert_eq!(
			council.members, old_council,
			"Members should be {:?} but are {:?}",
			old_council, council.members
		);
		assert_eq!(
			council.threshold, expected_threshold,
			"Threshold should be {} but is {}",
			expected_threshold, council.threshold
		);
		Ok(())
	}
}
