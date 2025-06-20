use sp_std::vec::Vec;

use super::{primitives::NonemptyContinuousHeaders, BHWTypes, HeightWitnesserProperties};
use crate::electoral_systems::state_machine::consensus::{
	ConsensusMechanism, MultipleVotes, StagedConsensus, StagedVote, SuccessThreshold,
	SupermajorityConsensus,
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
	type Settings = (SuccessThreshold, HeightWitnesserProperties<T>);

	fn insert_vote(&mut self, vote: Self::Vote) {
		self.votes.push(vote);
	}

	fn check_consensus(&self, settings: &Self::Settings) -> Option<Self::Result> {
		let (threshold, properties) = settings;

		if properties.witness_from_index == Default::default() {
			// This is the case for finding an appropriate block number to start witnessing from

			let mut consensus: MultipleVotes<SupermajorityConsensus<_>> = Default::default();

			for vote in &self.votes {
				consensus.insert_vote(vote.headers.iter().map(Clone::clone).collect())
			}

			consensus
				.check_consensus(threshold)
				.map(|result| NonemptyContinuousHeaders { headers: [result].into_iter().collect() })
		} else {
			// This is the actual consensus finding, once the engine is running

			let mut consensus: StagedConsensus<SupermajorityConsensus<Self::Vote>, usize> =
				StagedConsensus::new();

			for mut vote in self.votes.clone() {
				// we count a given vote as multiple votes for all nonempty subchains,
				// the longest subchain that achieves consensus wins
				while !vote.headers.is_empty() {
					consensus.insert_vote(StagedVote {
						priority: vote.headers.len(),
						vote: vote.clone(),
					});
					vote.headers.pop_back();
				}
			}

			consensus.check_consensus(threshold).inspect(|result| {
				log::info!(
					"(witness_from: {:?}): successful consensus for ranges: {:?}..={:?}",
					properties,
					result.headers.front(),
					result.headers.back()
				);
			})
		}
	}
}
