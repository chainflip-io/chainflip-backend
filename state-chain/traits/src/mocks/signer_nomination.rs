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

use cf_primitives::ForeignChain;
use frame_support::{parameter_types, traits::Get};
use sp_std::collections::btree_set::BTreeSet;

use crate::{BroadcastNomination, EpochIndex, EpochInfo, ThresholdSignerNomination};

parameter_types! {
	pub storage ThresholdNominees: Option<BTreeSet<u64>> = None;
	pub storage LastNominee: Option<u64> = None;
}

pub struct MockNominator;

impl BroadcastNomination for MockNominator {
	type BroadcasterId = u64;

	/// Nominates the lowest-numbered nominee that is not excluded, or `None` if they are all
	/// excluded.
	fn nominate_broadcaster<C: Get<ForeignChain>, S>(
		_seed: S,
		exclude_ids: impl IntoIterator<Item = Self::BroadcasterId>,
	) -> Option<Self::BroadcasterId> {
		let excluded = BTreeSet::from_iter(exclude_ids);
		let nominee = Self::get_nominees()
			.unwrap()
			.into_iter()
			.find(|nominee| !excluded.contains(nominee));

		LastNominee::set(&nominee);

		nominee
	}
}

impl ThresholdSignerNomination for MockNominator {
	type SignerId = u64;

	fn threshold_nomination_with_seed<S>(
		_seed: S,
		_epoch_index: EpochIndex,
	) -> Option<BTreeSet<Self::SignerId>> {
		Self::get_nominees()
	}

	fn threshold_nomination_with_seed_from_candidates<S>(
		_seed: S,
		_candidates: BTreeSet<Self::SignerId>,
		_epoch_index: EpochIndex,
	) -> Option<BTreeSet<Self::SignerId>> {
		Self::get_nominees()
	}
}

// Remove some threadlocal + refcell complexity from test code
impl MockNominator {
	pub fn get_nominees() -> Option<BTreeSet<u64>> {
		ThresholdNominees::get()
	}

	pub fn set_nominees(nominees: Option<BTreeSet<u64>>) {
		ThresholdNominees::set(&nominees);
	}

	pub fn get_last_nominee() -> Option<u64> {
		LastNominee::get()
	}

	pub fn use_current_authorities_as_nominees<
		E: EpochInfo<ValidatorId = <Self as BroadcastNomination>::BroadcasterId>,
	>() {
		Self::set_nominees(Some(BTreeSet::from_iter(E::current_authorities())));
	}
}
