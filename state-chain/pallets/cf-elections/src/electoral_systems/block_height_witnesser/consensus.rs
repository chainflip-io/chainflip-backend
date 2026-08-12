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
use sp_std::{collections::btree_set::BTreeSet, vec::Vec};

use super::{primitives::NonemptyContinuousHeaders, BHWTypes, HeightWitnesserProperties};
use crate::electoral_systems::state_machine::consensus::{
	ConsensusMechanism, StagedConsensus, StagedVote, SuccessThreshold, SupermajorityConsensus,
};

pub struct BlockHeightWitnesserConsensus<T: BHWTypes> {
	votes: Vec<NonemptyContinuousHeaders<T::Chain>>,
}

impl<T: BHWTypes> Default for BlockHeightWitnesserConsensus<T> {
	fn default() -> Self {
		Self { votes: Default::default() }
	}
}

impl<T: BHWTypes> ConsensusMechanism for BlockHeightWitnesserConsensus<T> {
	type Vote = NonemptyContinuousHeaders<T::Chain>;
	type Result = NonemptyContinuousHeaders<T::Chain>;
	type Settings = (SuccessThreshold, HeightWitnesserProperties<T::Chain>);

	fn insert_vote(&mut self, vote: Self::Vote) {
		self.votes.push(vote);
	}

	fn check_consensus(&self, settings: &Self::Settings) -> Option<Self::Result> {
		let (threshold, properties) = settings;

		if properties.witness_from_index == Default::default() {
			// This is the case for finding an appropriate block number to start witnessing from

			let mut consensus: SupermajorityConsensus<_> = Default::default();

			for vote in &self.votes {
				// we have to make sure that a single voter can't submit the same header multiple
				// times (and thus effectively gets multiple votes), so we reduce to just the
				// unique headers submitted
				let unique_headers = vote.get_headers().into_iter().collect::<BTreeSet<_>>();
				for header in unique_headers {
					consensus.insert_vote(header);
				}
			}

			consensus.check_consensus(threshold).map(NonemptyContinuousHeaders::new)
		} else {
			// This is the actual consensus finding, once the engine is running

			let mut consensus: StagedConsensus<SupermajorityConsensus<Self::Vote>, usize> =
				StagedConsensus::new();

			for mut vote in self.votes.clone() {
				// we count a given vote as multiple votes for all nonempty subchains,
				// the longest subchain that achieves consensus wins
				while vote.len() > 1 {
					consensus.insert_vote(StagedVote { priority: vote.len(), vote: vote.clone() });
					vote.safe_pop_back();
				}
				consensus.insert_vote(StagedVote { priority: 1, vote: vote.clone() });
			}

			consensus.check_consensus(threshold).inspect(|result| {
				log::debug!(
					"(witness_from: {:?}): successful consensus for ranges: {:?}..={:?}",
					properties,
					result.first(),
					result.last()
				);
			})
		}
	}

	fn vote_as_consensus(vote: &Self::Vote) -> Self::Result {
		vote.clone()
	}

	#[cfg(test)]
	fn is_supported_by_vote(consensus: &Self::Result, vote: &Self::Vote) -> bool {
		consensus.get_headers().iter().all(|header| vote.get_headers().contains(header))
	}

	#[cfg(test)]
	fn get_success_threshold(settings: &Self::Settings) -> &SuccessThreshold {
		&settings.0
	}
}

#[test]
fn test_bhw_consensus() {
	use proptest::{
		prelude::Arbitrary,
		strategy::{LazyJust, Strategy},
	};

	type Types = crate::electoral_systems::state_machine::core::TypesFor<(u8, bool, Vec<()>)>;

	BlockHeightWitnesserConsensus::<Types>::check_consensus_is_always_supported_by_success_threshold_votes(
		file!(),
		3,
		LazyJust::new(|| {
			(
				SuccessThreshold { success_threshold: 3 },
				HeightWitnesserProperties { witness_from_index: 0 },
			)
		}),
		(0, 10),
	);

	BlockHeightWitnesserConsensus::<
		crate::electoral_systems::state_machine::core::TypesFor<(u8, bool, ())>,
	>::check_consensus_is_always_supported_by_success_threshold_votes(
		file!(),
		3,
		u8::arbitrary().prop_map(|witness_from_index| {
			(
				SuccessThreshold { success_threshold: 3 },
				HeightWitnesserProperties { witness_from_index },
			)
		}),
		(0, 10),
	);
}
