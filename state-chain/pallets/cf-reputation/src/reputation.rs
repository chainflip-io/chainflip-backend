use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	pallet_prelude::Member,
	sp_runtime::traits::{AtLeast32BitUnsigned, Saturating},
	DebugNoBound, DefaultNoBound, Parameter,
};
use scale_info::TypeInfo;
use sp_std::fmt::Debug;

pub type ReputationPoints = i32;

/// Reputation of a node
#[derive(
	Encode, Decode, DebugNoBound, DefaultNoBound, TypeInfo, MaxEncodedLen, Clone, PartialEq, Eq,
)]
#[scale_info(skip_type_params(P))]
#[codec(mel_bound(P: ReputationParameters))]
pub struct ReputationTracker<P: ReputationParameters> {
	pub online_blocks: P::BlockNumber,
	pub reputation_points: ReputationPoints,
}

pub trait ReputationParameters {
	type BlockNumber: Member
		+ Parameter
		+ MaxEncodedLen
		+ AtLeast32BitUnsigned
		+ Copy
		+ Debug
		+ Default;

	// This is an on-chain constant
	fn bounds() -> (ReputationPoints, ReputationPoints);
	fn accrual_rate() -> (ReputationPoints, Self::BlockNumber);
}

impl<P: ReputationParameters> ReputationTracker<P> {
	/// Validators are rewarded for remaining online. We count the number of blocks they
	/// have been online for, and periodically convert this into reputation according to
	/// the accrual rate.
	pub fn boost_reputation(&mut self, block_since_last_hearbeat: P::BlockNumber) {
		self.online_blocks.saturating_accrue(block_since_last_hearbeat);
		let (reward, per_blocks) = P::accrual_rate();
		while self.online_blocks >= per_blocks {
			self.online_blocks.saturating_reduce(per_blocks);
			self.reputation_points.saturating_accrue(reward);
			self.clamp();
		}
	}

	/// Deducts reputation. Reputation is deducted for committing an offence.
	pub fn deduct_reputation(&mut self, points: ReputationPoints) {
		self.reputation_points.saturating_reduce(points);
		self.clamp();
	}

	/// Clamp the reputation points to the given bounds.
	fn clamp(&mut self) {
		let (floor, ceiling) = P::bounds();
		self.reputation_points = self.reputation_points.clamp(floor, ceiling);
	}
}

#[cfg(test)]
mod test_reputation {
	use super::*;

	#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
	pub struct TestParams<const REWARD: ReputationPoints, const RATE: u32>;

	const BOUNDS: (ReputationPoints, ReputationPoints) = (-5, 5);

	impl<const REWARD: ReputationPoints, const RATE: u32> ReputationParameters
		for TestParams<REWARD, RATE>
	{
		type BlockNumber = u32;

		fn bounds() -> (ReputationPoints, ReputationPoints) {
			BOUNDS
		}

		fn accrual_rate() -> (ReputationPoints, Self::BlockNumber) {
			(REWARD, RATE)
		}
	}

	#[test]
	fn test_reputation_accrual_bounds() {
		let mut rep = ReputationTracker::<TestParams<1, 20>>::default();
		assert_eq!(rep.reputation_points, 0);
		assert_eq!(rep.online_blocks, 0);

		rep.boost_reputation(70);
		assert_eq!(rep.reputation_points, 3);
		assert_eq!(rep.online_blocks, 10);
		rep.boost_reputation(10);
		assert_eq!(rep.reputation_points, 4);
		assert_eq!(rep.online_blocks, 0);
		rep.boost_reputation(10);
		assert_eq!(rep.reputation_points, 4);
		assert_eq!(rep.online_blocks, 10);
		rep.boost_reputation(1000);
		assert_eq!(rep.reputation_points, 5);
		assert_eq!(rep.online_blocks, 10);

		rep.deduct_reputation(1);
		assert_eq!(rep.reputation_points, 4);
		assert_eq!(rep.online_blocks, 10);

		rep.deduct_reputation(100);
		assert_eq!(rep.reputation_points, -5);
		assert_eq!(rep.online_blocks, 10);
	}
}
