use core::time;

use enum_iterator::all;

use crate::electoral_systems::{
	oracle_price::{
		primitives::{compute_median, Aggregated, Aggregation, UnixTime},
		state_machine::{
			ExternalChainBlockQueried, ExternalChainState, ExternalChainStateVote, OPTypes,
			PriceQuery,
		},
	},
	state_machine::common_imports::*,
};

#[derive_where(Default; )]
pub struct OraclePriceConsensus<T: OPTypes> {
	votes: Vec<ExternalChainStateVote<T>>,
	// blocks: Vec<ExternalChainBlockQueried>,
	// timestamps: AggregatedConsensus<T::Aggregation, UnixTime, PriceQuery<T>>,
}

impl<T: OPTypes> ConsensusMechanism for OraclePriceConsensus<T> {
	type Vote = ExternalChainStateVote<T>;
	type Result = ExternalChainState<T>;
	type Settings = (SuccessThreshold, PriceQuery<T>);

	fn insert_vote(&mut self, vote: Self::Vote) {
		self.votes.push(vote);
	}

	fn check_consensus(&self, (threshold, query): &Self::Settings) -> Option<Self::Result> {
		if self.votes.len() > threshold.success_threshold as usize {
			Some(ExternalChainState {
				block: compute_median(
					self.votes
						.iter()
						.map(|vote| vote.block.clone())
						.filter(|block| block.chain() == query.chain)
						.collect(),
				),
				timestamp: T::Aggregation::compute(
					&self.votes.iter().map(|vote| vote.timestamp.clone()).collect::<Vec<_>>(),
				),
				price: all::<T::Asset>()
					.map(|asset| {
						(
							asset.clone(),
							T::Aggregation::compute(
								&self
									.votes
									.iter()
									.filter_map(|vote| vote.price.get(&asset))
									.cloned()
									.collect::<Vec<_>>(),
							),
						)
					})
					.collect(),
			})
		} else {
			None
		}
	}

	fn vote_as_consensus(vote: &Self::Vote) -> Self::Result {
		let ExternalChainStateVote { block, timestamp, price } = vote;
		ExternalChainState {
			block: block.clone(),
			timestamp: T::Aggregation::compute(&[timestamp.clone()]),
			price: price
				.into_iter()
				.map(|(asset, price)| (asset.clone(), T::Aggregation::compute(&[price.clone()])))
				.collect(),
		}
	}
}
