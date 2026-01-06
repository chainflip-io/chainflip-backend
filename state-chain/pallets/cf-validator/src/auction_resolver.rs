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

use core::cmp::min;
use frame_support::sp_runtime::traits::AtLeast32BitUnsigned;

use crate::*;
use serde::{Deserialize, Serialize};

#[derive(
	Copy,
	Clone,
	RuntimeDebug,
	Default,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
)]
pub struct SetSizeMaximisingAuctionResolver {
	current_size: u32,
	parameters: SetSizeParameters,
}

#[derive(
	Copy,
	Clone,
	RuntimeDebug,
	Default,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
	Serialize,
	Deserialize,
)]
pub struct SetSizeParameters {
	pub min_size: u32,
	pub max_size: u32,
	pub max_expansion: u32,
}

#[derive(
	Copy,
	Clone,
	RuntimeDebug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
)]
pub enum AuctionError {
	/// Parameters must make sense ie. min <= max. And zero is not a valid size.
	InvalidParameters,
	/// The ranges defined by the absolute and relative size bounds must overlap.
	InconsistentRanges,
	/// Not enough bidders to satisfy the set size bounds.
	NotEnoughBidders,
}

/// The outcome of a successful auction.
#[derive(
	PartialEq,
	Eq,
	Clone,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	RuntimeDebug,
	Serialize,
	Deserialize,
)]
pub struct AuctionOutcome<Id, Amount> {
	/// The auction winners, sorted by descending bid.
	pub winners: Vec<Id>,
	/// The resulting bond for the next epoch.
	pub bond: Amount,
}

impl<Id, Amount> AuctionOutcome<Id, Amount> {
	pub fn map_ids<T>(self, f: impl Fn(Id) -> T) -> AuctionOutcome<T, Amount> {
		AuctionOutcome { winners: self.winners.into_iter().map(f).collect(), bond: self.bond }
	}
}

impl<T: Config> From<AuctionError> for Error<T> {
	fn from(err: AuctionError) -> Self {
		match err {
			AuctionError::InvalidParameters => Error::<T>::InvalidAuctionParameters,
			AuctionError::InconsistentRanges => Error::<T>::InconsistentRanges,
			AuctionError::NotEnoughBidders => Error::<T>::NotEnoughBidders,
		}
	}
}

impl SetSizeMaximisingAuctionResolver {
	pub fn try_new(
		current_size: u32,
		parameters @ SetSizeParameters { min_size, max_size, max_expansion }: SetSizeParameters,
	) -> Result<Self, AuctionError> {
		ensure!(
			min_size > 0 &&
				min_size <= max_size &&
				current_size.saturating_add(max_expansion) >= min_size,
			AuctionError::InvalidParameters
		);
		Ok(Self { current_size, parameters })
	}

	pub fn resolve_auction<CandidateId: Clone, BidAmount: Copy + AtLeast32BitUnsigned>(
		&self,
		mut auction_candidates: Vec<Bid<CandidateId, BidAmount>>,
	) -> Result<AuctionOutcome<CandidateId, BidAmount>, AuctionError> {
		ensure!(auction_candidates.len() as u32 >= self.parameters.min_size, {
			log::warn!(
				"[cf-auction] not enough auction candidates. {} < {}",
				auction_candidates.len(),
				self.parameters.min_size
			);
			AuctionError::NotEnoughBidders
		});

		let target_size = min(
			self.parameters.max_size,
			self.current_size.saturating_add(self.parameters.max_expansion),
		);

		auction_candidates.sort_unstable_by_key(|&Bid { amount, .. }| Reverse(amount));

		let losers =
			auction_candidates.split_off(min(target_size as usize, auction_candidates.len()));
		let bond = auction_candidates
			.last()
			.map(|bid| bid.amount)
			.expect("Can't run auction with no candidates, and candidates must be funded > 0.");
		let winners = auction_candidates.into_iter().map(|bid| bid.bidder_id).collect();

		debug_assert!(losers.is_sorted_by_key(|&Bid { amount, .. }| Reverse(amount)));

		Ok(AuctionOutcome { winners, bond })
	}
}

#[cfg(test)]
mod test_auction_resolution {
	use super::*;
	use cf_traits::Bid;

	#[test]
	fn test_parameter_validation() {
		// Normal case.
		assert!(SetSizeMaximisingAuctionResolver::try_new(
			120,
			SetSizeParameters { min_size: 3, max_size: 150, max_expansion: 15 }
		)
		.is_ok());

		// Forcing the set size to contract to a single-value range.
		assert!(SetSizeMaximisingAuctionResolver::try_new(
			10,
			SetSizeParameters { min_size: 5, max_size: 5, max_expansion: 0 }
		)
		.is_ok());

		// Forcing the set size to expand to a single-value range.
		assert!(SetSizeMaximisingAuctionResolver::try_new(
			10,
			SetSizeParameters { min_size: 15, max_size: 15, max_expansion: 10 }
		)
		.is_ok());

		// Min size can't be greater than max size.
		assert!(SetSizeMaximisingAuctionResolver::try_new(
			10,
			SetSizeParameters { min_size: 10, max_size: 9, max_expansion: 10 }
		)
		.is_err());

		// Expansion range must overlap, contraction is unbounded.
		assert!(SetSizeMaximisingAuctionResolver::try_new(
			100,
			SetSizeParameters { min_size: 5, max_size: 10, max_expansion: 10 }
		)
		.is_ok());
		assert!(SetSizeMaximisingAuctionResolver::try_new(
			100,
			SetSizeParameters { min_size: 140, max_size: 150, max_expansion: 10 }
		)
		.is_err());
		assert!(SetSizeMaximisingAuctionResolver::try_new(
			100,
			SetSizeParameters { min_size: 110, max_size: 150, max_expansion: 10 }
		)
		.is_ok());
	}

	macro_rules! check_auction_resolution_invariants {
		($candidates:ident, $resolver:ident, $outcome:ident) => {
			let AuctionOutcome { winners, .. } = $outcome;

			assert!(
				winners.len() as u32 >= $resolver.parameters.min_size,
				"Set size cannot be lower than min_size."
			);
			assert!(
				winners.len() as u32 <= $resolver.parameters.max_size,
				"Set size cannot be higher than max_size."
			);

			assert!(
				winners.len() as u32 <= $resolver.current_size + $resolver.parameters.max_expansion,
				"max_expansion constraint violated."
			);
		};
	}

	#[test]
	fn set_size_expands_to_global_limit() {
		const CURRENT_SIZE: u32 = 50;
		const MAX_SIZE: u32 = 100;
		const AUCTION_PARAMETERS: SetSizeParameters =
			SetSizeParameters { min_size: 5, max_size: MAX_SIZE, max_expansion: 100 };
		let auction_resolver =
			SetSizeMaximisingAuctionResolver::try_new(CURRENT_SIZE, AUCTION_PARAMETERS).unwrap();

		// All candidates bid the same amount.
		let candidates = (0u64..1000)
			.map(|bidder_id| Bid { bidder_id, amount: 100u128 })
			.collect::<Vec<_>>();

		let outcome = auction_resolver.resolve_auction(candidates.clone()).unwrap();

		assert_eq!(outcome.winners.len() as u32, MAX_SIZE);

		check_auction_resolution_invariants!(candidates, auction_resolver, outcome);
	}

	#[test]
	fn set_size_expands_to_expansion_limit() {
		const CURRENT_SIZE: u32 = 50;
		const MAX_EXPANSION: u32 = 10;
		const AUCTION_PARAMETERS: SetSizeParameters =
			SetSizeParameters { min_size: 5, max_size: 100, max_expansion: MAX_EXPANSION };
		let auction_resolver =
			SetSizeMaximisingAuctionResolver::try_new(CURRENT_SIZE, AUCTION_PARAMETERS).unwrap();

		// All candidates bid the same amount.
		let candidates = (0u64..1000)
			.map(|bidder_id| Bid { bidder_id, amount: 100u128 })
			.collect::<Vec<_>>();

		let outcome = auction_resolver.resolve_auction(candidates.clone()).unwrap();

		assert_eq!(outcome.winners.len() as u32, CURRENT_SIZE + MAX_EXPANSION);

		check_auction_resolution_invariants!(candidates, auction_resolver, outcome);
	}

	#[test]
	fn losers_are_returned_in_order_of_descending_bid_amount() {
		const CURRENT_SIZE: u32 = 5;
		const MAX_EXPANSION: u32 = 10;
		const AUCTION_PARAMETERS: SetSizeParameters = SetSizeParameters {
			min_size: CURRENT_SIZE,
			max_size: CURRENT_SIZE,
			max_expansion: MAX_EXPANSION,
		};
		let auction_resolver =
			SetSizeMaximisingAuctionResolver::try_new(CURRENT_SIZE, AUCTION_PARAMETERS).unwrap();

		use nanorand::{Rng, WyRand};

		let candidates = 0u64..100;
		let mut bids: Vec<_> = (100_u64..200).collect();
		WyRand::new_seed(4).shuffle(&mut bids);

		let candidates: Vec<_> = candidates
			.zip(bids)
			.map(|(bidder_id, amount)| Bid { bidder_id, amount })
			.collect();

		let outcome = auction_resolver.resolve_auction(candidates).unwrap();

		assert_eq!(outcome.bond, 195);
		assert_eq!(outcome.winners.len(), CURRENT_SIZE as usize);
	}

	#[test]
	fn losers_are_cut_off_at_cutoff_percentage() {
		const CURRENT_SIZE: u32 = 100;
		const MAX_EXPANSION: u32 = 0;
		const NUM_LOSERS: u32 = 50;
		const AUCTION_PARAMETERS: SetSizeParameters = SetSizeParameters {
			min_size: CURRENT_SIZE,
			max_size: CURRENT_SIZE,
			max_expansion: MAX_EXPANSION,
		};
		let auction_resolver =
			SetSizeMaximisingAuctionResolver::try_new(CURRENT_SIZE, AUCTION_PARAMETERS).unwrap();

		use nanorand::{Rng, WyRand};

		let candidates = 0u32..(CURRENT_SIZE + NUM_LOSERS);
		let mut bids: Vec<_> = (0..(CURRENT_SIZE + NUM_LOSERS)).collect();
		WyRand::new_seed(4).shuffle(&mut bids);

		let candidates: Vec<_> = candidates
			.zip(bids)
			.map(|(bidder_id, amount)| Bid { bidder_id, amount })
			.collect();

		let outcome = auction_resolver.resolve_auction(candidates).unwrap();

		assert_eq!(outcome.bond, NUM_LOSERS);
		assert_eq!(outcome.winners.len(), CURRENT_SIZE as usize);
	}
}
