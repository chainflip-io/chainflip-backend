use enum_iterator::all;

use crate::electoral_systems::{
	oracle_price::{
		primitives::Aggregation,
		state_machine::{AssetResponse, ExternalChainStateVote, OPTypes, PriceQuery},
	},
	state_machine::common_imports::*,
};

#[derive_where(Default; )]
pub struct OraclePriceConsensus<T: OPTypes> {
	votes: Vec<ExternalChainStateVote<T>>,
}

impl<T: OPTypes> ConsensusMechanism for OraclePriceConsensus<T> {
	type Vote = ExternalChainStateVote<T>;
	type Result = BTreeMap<T::AssetPair, AssetResponse<T>>;
	type Settings = (SuccessThreshold, PriceQuery<T>);

	fn insert_vote(&mut self, vote: Self::Vote) {
		self.votes.push(vote);
	}

	fn check_consensus(&self, (threshold, _query): &Self::Settings) -> Option<Self::Result> {
		if self.votes.len() >= threshold.success_threshold as usize {
			Some(
				all::<T::AssetPair>()
					.filter_map(|asset| {
						Some((
							asset.clone(),
							(
								T::Aggregation::compute(
									&self
										.votes
										.iter()
										.filter_map(|vote| vote.price.get(&asset))
										.map(|(timestamp, _price)| timestamp)
										.cloned()
										.collect::<Vec<_>>(),
								)?,
								T::Aggregation::compute(
									&self
										.votes
										.iter()
										.filter_map(|vote| vote.price.get(&asset))
										.map(|(_timestamp, price)| price)
										.cloned()
										.collect::<Vec<_>>(),
								)?,
							),
						))
					})
					.map(|(asset, (timestamp, price))| (asset, AssetResponse { timestamp, price }))
					.collect(),
			)
		} else {
			None
		}
	}

	fn vote_as_consensus(vote: &Self::Vote) -> Self::Result {
		let ExternalChainStateVote { price } = vote;
		price
			.iter()
			.map(|(asset, (timestamp, price))| {
				(
					asset.clone(),
					AssetResponse {
						timestamp: T::Aggregation::single(timestamp),
						price: T::Aggregation::single(price),
					},
				)
			})
			.collect()
	}
}
