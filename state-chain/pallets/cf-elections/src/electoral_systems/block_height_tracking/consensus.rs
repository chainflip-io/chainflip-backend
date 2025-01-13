use cf_chains::witness_period::BlockZero;
use sp_std::{collections::vec_deque::VecDeque, vec::Vec};

use super::{
	primitives::validate_vote_and_height, BlockHeightTrackingProperties, BlockHeightTrackingTypes,
	state_machine::InputHeaders,
};
use crate::electoral_systems::state_machine::consensus::{
	ConsensusMechanism, StagedConsensus, SupermajorityConsensus, Threshold,
};

pub struct BlockHeightTrackingConsensus<T: BlockHeightTrackingTypes> {
	votes: Vec<InputHeaders<T>>,
}

impl<T: BlockHeightTrackingTypes> Default for BlockHeightTrackingConsensus<T> {
	fn default() -> Self {
		Self { votes: Default::default() }
	}
}

impl<T: BlockHeightTrackingTypes> ConsensusMechanism for BlockHeightTrackingConsensus<T> {
	type Vote = InputHeaders<T>;
	type Result = InputHeaders<T>;
	type Settings = (Threshold, BlockHeightTrackingProperties<T::ChainBlockNumber>);

	fn insert_vote(&mut self, vote: Self::Vote) {
		self.votes.push(vote);
	}

	fn check_consensus(&self, settings: &Self::Settings) -> Option<Self::Result> {
		// let num_authorities = consensus_votes.num_authorities();

		let (threshold, properties) = settings;

		if properties.witness_from_index.is_zero() {
			// This is the case for finding an appropriate block number to start witnessing from

			let mut consensus: SupermajorityConsensus<_> = SupermajorityConsensus::default();

			for vote in &self.votes {
				// we currently only count votes consisting of a single block height
				// there has to be a supermajority voting for the exact same header
				if vote.0.len() == 1 {
					consensus.insert_vote(vote.0[0].clone())
				}
			}

			consensus
				.check_consensus(&threshold)
				.map(|result| {
					let mut headers = VecDeque::new();
					headers.push_back(result);
					InputHeaders(headers)
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
				// ensure that the vote is valid
				if let Err(err) = validate_vote_and_height(properties.witness_from_index, &vote.0) {
					log::warn!("received invalid vote: {err:?} ");
					continue;
				}

				// we count a given vote as multiple votes for all nonempty subchains
				while vote.0.len() > 0 {
					consensus.insert_vote((vote.0.len(), vote.clone()));
					vote.0.pop_back();
				}
			}

			consensus.check_consensus(&threshold).map(|result| {
				log::info!(
					"(witness_from: {:?}): successful consensus for ranges: {:?}..={:?}",
					properties,
					result.0.front(),
					result.0.back()
				);
				result
			})
		}
	}
}
