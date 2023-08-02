use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{pallet_prelude::Member, Parameter};
use scale_info::TypeInfo;
use frame_support::sp_runtime::traits::{AtLeast32BitUnsigned, Saturating, Zero};
use sp_std::fmt::Debug;

pub type ReputationPoints = i32;

/// Reputation of a node
#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, PartialEq, Eq)]
#[scale_info(skip_type_params(P))]
#[codec(mel_bound(P: ReputationParameters))]
pub struct ReputationTracker<P: ReputationParameters> {
	pub online_credits: P::OnlineCredits,
	pub reputation_points: ReputationPoints,
}

impl<P: ReputationParameters> Default for ReputationTracker<P> {
	fn default() -> Self {
		Self { online_credits: Default::default(), reputation_points: Default::default() }
	}
}

impl<P: ReputationParameters> Debug for ReputationTracker<P> {
	fn fmt(&self, f: &mut sp_std::fmt::Formatter<'_>) -> sp_std::fmt::Result {
		f.debug_struct("ReputationTracker")
			.field("online_credits", &self.online_credits)
			.field("reputation_points", &self.reputation_points)
			.finish()
	}
}

pub trait ReputationParameters {
	type OnlineCredits: Member
		+ Parameter
		+ MaxEncodedLen
		+ AtLeast32BitUnsigned
		+ Copy
		+ Debug
		+ Default;

	// This is an on-chain constant
	fn bounds() -> (ReputationPoints, ReputationPoints);
	fn accrual_rate() -> (ReputationPoints, Self::OnlineCredits);
}

impl<P: ReputationParameters> ReputationTracker<P> {
	/// Validators are rewarded with [OnlineCreditsFor] for remaining online. The credits are
	/// automatically converted to reputation points according to
	/// some conversion ratio (ie. the [AccrualRatio]).
	pub fn boost_reputation(&mut self, online_credit_reward: P::OnlineCredits) {
		self.online_credits.saturating_accrue(online_credit_reward);
		let (reward, per_credits) = P::accrual_rate();
		while self.online_credits >= per_credits {
			self.online_credits.saturating_reduce(per_credits);
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

	/// Reset online credits to zero.
	pub fn reset_online_credits(&mut self) {
		self.online_credits = Zero::zero();
	}

	/// Reset Reputation to zero.
	pub fn reset_reputation(&mut self) {
		self.reputation_points = Zero::zero();
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
		type OnlineCredits = u32;

		fn bounds() -> (ReputationPoints, ReputationPoints) {
			BOUNDS
		}

		fn accrual_rate() -> (ReputationPoints, Self::OnlineCredits) {
			(REWARD, RATE)
		}
	}

	#[test]
	fn test_reputation_accrual_bounds() {
		let mut rep = ReputationTracker::<TestParams<1, 20>>::default();
		assert_eq!(rep.reputation_points, 0);
		assert_eq!(rep.online_credits, 0);

		rep.boost_reputation(70);
		assert_eq!(rep.reputation_points, 3);
		assert_eq!(rep.online_credits, 10);
		rep.boost_reputation(10);
		assert_eq!(rep.reputation_points, 4);
		assert_eq!(rep.online_credits, 0);
		rep.boost_reputation(10);
		assert_eq!(rep.reputation_points, 4);
		assert_eq!(rep.online_credits, 10);
		rep.boost_reputation(1000);
		assert_eq!(rep.reputation_points, 5);
		assert_eq!(rep.online_credits, 10);

		rep.deduct_reputation(1);
		assert_eq!(rep.reputation_points, 4);
		assert_eq!(rep.online_credits, 10);

		rep.deduct_reputation(100);
		assert_eq!(rep.reputation_points, -5);
		assert_eq!(rep.online_credits, 10);
	}
}
