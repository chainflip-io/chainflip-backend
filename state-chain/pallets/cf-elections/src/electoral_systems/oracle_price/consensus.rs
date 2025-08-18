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

use enum_iterator::all;

use crate::electoral_systems::{
	oracle_price::{
		primitives::{compute_aggregated, Aggregated},
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
						let single_asset_votes: (Vec<_>, Vec<_>) = self
							.votes
							.iter()
							.filter_map(|vote| vote.price.get(&asset))
							.cloned()
							.unzip();
						if single_asset_votes.0.len() < threshold.success_threshold as usize ||
							single_asset_votes.1.len() < threshold.success_threshold as usize
						{
							return None;
						}

						Some((
							asset.clone(),
							(
								compute_aggregated(single_asset_votes.0)?,
								compute_aggregated(single_asset_votes.1)?,
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
						timestamp: Aggregated::from_single_value(*timestamp),
						price: Aggregated::from_single_value(price.clone()),
					},
				)
			})
			.collect()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::electoral_systems::oracle_price::state_machine::{
		tests::MockTypes, ExternalChainStateVote, PriceQuery,
	};
	use proptest::collection::vec;

	proptest! {
		#[test]
		fn fuzzy_consensus(votes in vec(any::<ExternalChainStateVote<MockTypes>>(), 0..30), success_threshold in 0..40u32, price_query in any::<PriceQuery<MockTypes>>()) {
			let mut consensus: OraclePriceConsensus<MockTypes> = Default::default();
			for vote in votes {
				consensus.insert_vote(vote);
			}
			let _ = consensus.check_consensus(&
				(SuccessThreshold { success_threshold }, price_query)
			);
		}
	}
}
