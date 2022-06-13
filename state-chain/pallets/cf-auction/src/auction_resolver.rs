use core::cmp::{max, min};

use sp_runtime::traits::AtLeast32BitUnsigned;

use crate::*;

#[derive(
	Copy, Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen,
)]
pub struct DynamicSetSizeAuctionResolver {
	current_size: u32,
	parameters: DynamicSetSizeParameters,
}

#[derive(
	Copy, Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen,
)]
pub struct DynamicSetSizeParameters {
	pub min_size: u32,
	pub max_size: u32,
	pub max_contraction: u32,
	pub max_expansion: u32,
}

#[derive(Copy, Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum AuctionError {
	/// Parameters must make sense ie. min <= max. And zero is not a valid size.
	InvalidParameters,
	/// The ranges defined by the absolute and relative size bounds must overlap.
	InconsistentRanges,
	/// Not enough bidders to satisfy the set size bounds.
	NotEnoughBidders,
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

impl DynamicSetSizeAuctionResolver {
	pub fn try_new(
		current_size: u32,
		parameters @ DynamicSetSizeParameters { min_size, max_size, max_contraction, max_expansion }: DynamicSetSizeParameters,
	) -> Result<Self, AuctionError> {
		ensure!(min_size > 0, AuctionError::InvalidParameters);
		ensure!(min_size <= max_size, AuctionError::InvalidParameters);
		ensure!(
			current_size.saturating_add(max_expansion) >= min_size &&
				current_size.saturating_sub(max_contraction) <= max_size,
			AuctionError::InconsistentRanges
		);
		Ok(Self { current_size, parameters })
	}

	pub fn resolve_auction<CandidateId: Clone, BidAmount: Copy + AtLeast32BitUnsigned>(
		&self,
		mut auction_candidates: Vec<(CandidateId, BidAmount)>,
	) -> Result<AuctionOutcome<CandidateId, BidAmount>, AuctionError> {
		ensure!(auction_candidates.len() as u32 >= self.parameters.min_size, {
			log::error!(
				"[cf-auction] not enough auction candidates. {} < {}",
				auction_candidates.len(),
				self.parameters.min_size
			);
			AuctionError::NotEnoughBidders
		});

		let (lower_bound_inclusive, upper_bound_inclusive) = (
			max(
				self.parameters.min_size,
				self.current_size.saturating_sub(self.parameters.max_contraction),
			),
			min(
				self.parameters.max_size,
				self.current_size.saturating_add(self.parameters.max_expansion),
			),
		);

		// These conditions should always be true if the parameters are valid, but debug_assert them
		// so that we can catch it during testing.
		debug_assert!(lower_bound_inclusive > 0);
		debug_assert!(upper_bound_inclusive >= lower_bound_inclusive);

		auction_candidates.sort_unstable_by_key(|&(_, amount)| Reverse(amount));

		let (index, (_, bond)) = auction_candidates
			.iter()
			.enumerate()
			.skip(lower_bound_inclusive as usize - 1)
			.take(1 + (upper_bound_inclusive - lower_bound_inclusive) as usize)
			// Choose the candidate that maximises total collateral (tcl).
			// If multiple entries result in the same tcl, take the largest set size.
			.max_by_key(|(index, &(_, amount))| (amount.saturating_mul((*index as u32 + 1).into()), *index))
			.expect(
				"We always have at least one candidate in the iteration since upper_bound >= lower_bound."
			);

		let (winning_bids, losing_bids) = auction_candidates.split_at(index + 1);
		let winners = winning_bids.iter().map(|(id, _)| id).cloned().collect();
		let losers = losing_bids.iter().cloned().map(Into::into).collect();

		Ok(AuctionOutcome { winners, losers, bond: *bond })
	}
}

#[cfg(test)]
mod test_auction_resolution {
	use super::*;

	use cf_traits::Bid;

	#[test]
	fn test_parameter_validation() {
		// Normal case.
		assert!(DynamicSetSizeAuctionResolver::try_new(
			120,
			DynamicSetSizeParameters {
				min_size: 3,
				max_size: 150,
				max_contraction: 10,
				max_expansion: 15,
			}
		)
		.is_ok());

		// Forcing the set size to a single-value range.
		assert!(DynamicSetSizeAuctionResolver::try_new(
			10,
			DynamicSetSizeParameters {
				min_size: 5,
				max_size: 5,
				max_contraction: 5,
				max_expansion: 0,
			}
		)
		.is_ok());

		// Min size can't be greater than max size.
		assert!(DynamicSetSizeAuctionResolver::try_new(
			10,
			DynamicSetSizeParameters {
				min_size: 10,
				max_size: 9,
				max_contraction: 10,
				max_expansion: 10,
			}
		)
		.is_err());

		// Ranges must overlap
		assert!(DynamicSetSizeAuctionResolver::try_new(
			100,
			DynamicSetSizeParameters {
				min_size: 5,
				max_size: 10,
				max_contraction: 10,
				max_expansion: 10,
			}
		)
		.is_err());
		assert!(DynamicSetSizeAuctionResolver::try_new(
			100,
			DynamicSetSizeParameters {
				min_size: 140,
				max_size: 150,
				max_contraction: 10,
				max_expansion: 10,
			}
		)
		.is_err());
		assert!(DynamicSetSizeAuctionResolver::try_new(
			100,
			DynamicSetSizeParameters {
				min_size: 5,
				max_size: 90,
				max_contraction: 10,
				max_expansion: 10,
			}
		)
		.is_ok());
		assert!(DynamicSetSizeAuctionResolver::try_new(
			100,
			DynamicSetSizeParameters {
				min_size: 110,
				max_size: 150,
				max_contraction: 10,
				max_expansion: 10,
			}
		)
		.is_ok());
	}

	macro_rules! check_auction_resolution_invariants {
		($candidates:ident, $resolver:ident, $outcome:ident) => {
			let AuctionOutcome { winners, losers, bond } = $outcome;

			assert_eq!(
				winners
					.iter()
					.chain(losers.iter().map(|bid| &bid.bidder_id))
					.cloned()
					.collect::<BTreeSet<_>>(),
				$candidates.iter().map(|(id, _)| id).cloned().collect::<BTreeSet<_>>()
			);

			assert!(winners.len() as u32 >= $resolver.parameters.min_size);
			assert!(winners.len() as u32 <= $resolver.parameters.max_size);
			assert!(
				winners.len() as u32 >=
					$resolver.current_size - $resolver.parameters.max_contraction
			);
			assert!(
				winners.len() as u32 <= $resolver.current_size + $resolver.parameters.max_expansion
			);

			for Bid { amount, .. } in losers.iter() {
				assert!(*amount <= bond);
			}
		};
	}

	fn gen_bid_profile(inflection_point: u64, falloff_slope: u128) -> impl Fn(u64) -> u128 {
		move |i| {
			if i < inflection_point {
				100u128
			} else {
				100u128.saturating_sub(i as u128 * falloff_slope)
			}
		}
	}

	#[test]
	fn set_size_expands_to_limit() {
		const CURRENT_SIZE: u32 = 50;
		const MAX_EXPANSION: u32 = 10;
		const AUCTION_PARAMETERS: DynamicSetSizeParameters = DynamicSetSizeParameters {
			min_size: 5,
			max_size: 100,
			max_contraction: 10,
			max_expansion: MAX_EXPANSION,
		};
		let auction_resolver =
			DynamicSetSizeAuctionResolver::try_new(CURRENT_SIZE, AUCTION_PARAMETERS).unwrap();

		// All candidates bid the same amount.
		let bid_profile = gen_bid_profile(100, 0);
		let candidates = (0u64..100).map(|i| (i, bid_profile(i))).collect::<Vec<_>>();

		let outcome = auction_resolver.resolve_auction(candidates.clone()).unwrap();

		assert_eq!(outcome.winners.len() as u32, CURRENT_SIZE + MAX_EXPANSION);

		check_auction_resolution_invariants!(candidates, auction_resolver, outcome);
	}

	#[test]
	fn set_size_converges_at_peak() {
		const CURRENT_SIZE: u32 = 50;
		const INFLECTION_POINT: u64 = 55;
		const AUCTION_PARAMETERS: DynamicSetSizeParameters = DynamicSetSizeParameters {
			min_size: 5,
			max_size: 100,
			max_contraction: 50,
			max_expansion: 50,
		};
		let auction_resolver =
			DynamicSetSizeAuctionResolver::try_new(CURRENT_SIZE, AUCTION_PARAMETERS).unwrap();

		// Constant bids up to the inflection point, zero thereafter.
		// This creates a tcl profile with a local peak at the inflection point.
		let bid_profile = gen_bid_profile(INFLECTION_POINT, 100);
		let candidates = (0u64..100).map(|i| (i, bid_profile(i))).collect::<Vec<_>>();

		let outcome = auction_resolver.resolve_auction(candidates.clone()).unwrap();

		assert_eq!(
			outcome.bond,
			100u128,
			"\nCandidate bids were: {:?}. \nOutcome was: {:?}. \nTcl: {:?}.",
			candidates,
			outcome,
			outcome.projected_total_collateral(),
		);
		assert_eq!(
			outcome.winners.len(),
			INFLECTION_POINT as usize,
			"\nCandidate bids were: {:?}. \nOutcome was: {:?}. \nTcl: {:?}.",
			candidates,
			outcome,
			outcome.projected_total_collateral(),
		);

		check_auction_resolution_invariants!(candidates, auction_resolver, outcome);
	}

	#[test]
	fn set_size_contracts_to_limit() {
		const CURRENT_SIZE: u32 = 50;
		const MAX_CONTRACTION: u32 = 10;
		const AUCTION_PARAMETERS: DynamicSetSizeParameters = DynamicSetSizeParameters {
			min_size: 5,
			max_size: 100,
			max_contraction: MAX_CONTRACTION,
			max_expansion: 50,
		};
		let auction_resolver =
			DynamicSetSizeAuctionResolver::try_new(CURRENT_SIZE, AUCTION_PARAMETERS).unwrap();

		// Constant bids up to the inflection point, rapidly decreasing thereafter.
		// However this time, the peak is lower than the max_contraction limit.
		let bid_profile = gen_bid_profile(30, 2);

		let candidates = (1u64..=100).map(|i| (i, bid_profile(i))).collect::<Vec<_>>();

		let outcome = auction_resolver.resolve_auction(candidates.clone()).unwrap();

		assert_eq!(
			outcome.bond,
			bid_profile((CURRENT_SIZE - MAX_CONTRACTION) as u64),
			"\nCandidate bids were: {:?}. \nOutcome was: {:?}. \nTcl: {:?}.",
			candidates,
			outcome,
			outcome.projected_total_collateral(),
		);
		assert_eq!(
			outcome.winners.len(),
			(CURRENT_SIZE - MAX_CONTRACTION) as usize,
			"\nCandidate bids were: {:?}. \nOutcome was: {:?}. \nTcl: {:?}.",
			candidates,
			outcome,
			outcome.projected_total_collateral(),
		);

		check_auction_resolution_invariants!(candidates, auction_resolver, outcome);
	}
}
