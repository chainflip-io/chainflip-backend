use cf_chains::witness_period::BlockZero;
use sp_std::{collections::vec_deque::VecDeque, vec::Vec};

use super::{primitives::NonemptyContinuousHeaders, HWTypes, HeightWitnesserProperties};
use crate::electoral_systems::state_machine::consensus::{
	ConsensusMechanism, MultipleVotes, StagedConsensus, SupermajorityConsensus, Threshold,
};

pub struct BlockHeightTrackingConsensus<T: HWTypes> {
	votes: Vec<NonemptyContinuousHeaders<T::Chain>>,
}

impl<T: HWTypes> Default for BlockHeightTrackingConsensus<T> {
	fn default() -> Self {
		Self { votes: Default::default() }
	}
}

impl<T: HWTypes> ConsensusMechanism for BlockHeightTrackingConsensus<T> {
	type Vote = NonemptyContinuousHeaders<T::Chain>;
	type Result = NonemptyContinuousHeaders<T::Chain>;
	type Settings = (Threshold, HeightWitnesserProperties<T>);

	fn insert_vote(&mut self, vote: Self::Vote) {
		self.votes.push(vote);
	}

	fn check_consensus(&self, settings: &Self::Settings) -> Option<Self::Result> {
		let (threshold, properties) = settings;

		if properties.witness_from_index.is_zero() {
			// This is the case for finding an appropriate block number to start witnessing from

			let mut consensus: MultipleVotes<SupermajorityConsensus<_>> = Default::default();

			for vote in &self.votes {
				consensus.insert_vote(vote.headers.iter().map(Clone::clone).collect())
			}

			consensus
				.check_consensus(threshold)
				.map(|result| {
					let mut headers = VecDeque::new();
					headers.push_back(result);
					NonemptyContinuousHeaders { headers }
				})
				.map(|result| {
					log::info!("block_height: initial consensus: {result:?}");
					result
				})
		} else {
			// This is the actual consensus finding, once the engine is running

			let mut consensus: StagedConsensus<SupermajorityConsensus<Self::Vote>, usize> =
				StagedConsensus::new();

			for mut vote in self.votes.clone() {
				// we count a given vote as multiple votes for all nonempty subchains
				while !vote.headers.is_empty() {
					consensus.insert_vote((vote.headers.len(), vote.clone()));
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
