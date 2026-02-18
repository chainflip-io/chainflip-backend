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
use frame_support::sp_runtime::traits::AtLeast32BitUnsigned;
use sp_std::collections::btree_set::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, Default)]
pub struct RotationState<Id, Amount> {
	pub primary_candidates: Vec<Id>,
	pub banned: BTreeSet<Id>,
	pub bond: Amount,
	pub new_epoch_index: EpochIndex,
}

impl<Id: Ord + Clone, Amount: AtLeast32BitUnsigned + Copy> RotationState<Id, Amount> {
	pub fn from_auction_outcome<T: Config>(
		AuctionOutcome { winners, bond, .. }: AuctionOutcome<Id, Amount>,
	) -> Self {
		RotationState {
			primary_candidates: winners,
			banned: Default::default(),
			bond,
			new_epoch_index: T::EpochInfo::epoch_index() + 1,
		}
	}

	pub fn ban(&mut self, new_banned: BTreeSet<Id>) {
		for id in new_banned {
			self.banned.insert(id);
		}
	}

	pub fn authority_candidates(&self) -> BTreeSet<Id> {
		self.primary_candidates
			.iter()
			.filter(|id| !self.banned.contains(id))
			.cloned()
			.collect()
	}

	pub fn num_primary_candidates(&self) -> u32 {
		self.primary_candidates.len() as u32
	}

	pub fn unbanned_current_authorities<T: Config + Chainflip<ValidatorId = Id>>(
		&self,
	) -> BTreeSet<Id> {
		Pallet::<T>::current_authorities()
			.into_iter()
			.filter(|id| !self.banned.contains(id))
			.collect()
	}
}

#[cfg(test)]
mod rotation_state_tests {
	use super::*;

	type Id = u64;
	type Amount = u128;

	#[test]
	fn banning_is_additive() {
		let mut rotation_state = RotationState::<Id, Amount> {
			primary_candidates: (0..10).collect(),
			banned: Default::default(),
			bond: 500,
			new_epoch_index: 2,
		};

		let first_ban = BTreeSet::from([8, 9, 7]);
		rotation_state.ban(first_ban.clone());
		assert_eq!(first_ban, rotation_state.banned);

		let second_ban = BTreeSet::from([1, 2, 3]);
		rotation_state.ban(second_ban.clone());
		assert_eq!(
			first_ban.union(&second_ban).cloned().collect::<BTreeSet<_>>(),
			rotation_state.banned
		);
	}

	#[test]
	fn authority_candidates_prefers_primaries_and_excludes_banned() {
		let rotation_state = RotationState::<Id, Amount> {
			primary_candidates: (0..10).collect(),
			banned: BTreeSet::from([1, 2, 4]),
			bond: 500,
			new_epoch_index: 2,
		};

		let candidates = rotation_state.authority_candidates();

		assert_eq!(candidates, BTreeSet::from([0, 3, 5, 6, 7, 8, 9]));
	}
}
