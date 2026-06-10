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
use crate::{
	electoral_systems::{
		block_height_witnesser::ChainBlockHashOf,
		state_machine::consensus::{ConsensusMechanism, SuccessThreshold, SupermajorityConsensus},
	},
	SharedDataHash,
};
use cf_runtime_utilities::log_or_panic;
use frame_support::Hashable;
use sp_std::collections::btree_map::BTreeMap;

use super::state_machine::{BWElectionProperties, BWTypes};

#[expect(clippy::type_complexity)]
pub struct BWConsensus<T: BWTypes> {
	pub consensus: SupermajorityConsensus<SharedDataHash>,
	pub data: BTreeMap<SharedDataHash, (T::BlockData, Option<ChainBlockHashOf<T::Chain>>)>,
	pub _phantom: sp_std::marker::PhantomData<T>,
}

impl<T: BWTypes> Default for BWConsensus<T> {
	fn default() -> Self {
		Self {
			consensus: Default::default(),
			data: Default::default(),
			_phantom: Default::default(),
		}
	}
}

impl<T: BWTypes> ConsensusMechanism for BWConsensus<T>
where
	(T::BlockData, Option<ChainBlockHashOf<T::Chain>>): Hashable,
{
	type Vote = (T::BlockData, Option<ChainBlockHashOf<T::Chain>>);
	type Result = (T::BlockData, Option<ChainBlockHashOf<T::Chain>>);
	type Settings = (SuccessThreshold, BWElectionProperties<T>);

	fn insert_vote(&mut self, vote: Self::Vote) {
		let vote_hash = SharedDataHash::of(&vote);
		self.data.entry(vote_hash).or_insert_with(|| vote.clone());
		self.consensus.insert_vote(vote_hash);
	}

	fn check_consensus(&self, settings: &Self::Settings) -> Option<Self::Result> {
		self.consensus.check_consensus(&settings.0).and_then(|consensus| {
			if let Some(data) = self.data.get(&consensus) {
				Some(data.clone())
			} else {
				log_or_panic!("Expected data to exist for hash");
				None
			}
		})
	}

	fn vote_as_consensus(vote: &Self::Vote) -> Self::Result {
		vote.clone()
	}
}
