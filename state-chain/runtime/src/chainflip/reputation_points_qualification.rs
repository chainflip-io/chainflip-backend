use crate::Runtime;
use cf_traits::QualifyNode;
use pallet_cf_reputation::Reputations;
use sp_runtime::AccountId32;
use sp_std::{cmp::min, vec::Vec};

// Checks authorities reputation points are greater than 0, but limits the number that can be
// disqualified due to this. To protect against huge set size drops.
pub struct ReputationPointsQualification;

impl QualifyNode<AccountId32> for ReputationPointsQualification {
	fn is_qualified(validator_id: &AccountId32) -> bool {
		let mut points = Reputations::<Runtime>::iter_values()
			.map(|r| r.reputation_points)
			.collect::<Vec<_>>();

		points.sort_unstable();

		// get the 33rd percentile of reputation points, and min with 0. This way in normal
		// operating scenarios where there might be a node on 2500 points one on -100 and the rest
		// on 2880. The cutoff will be 0, and the node on -100 will be disqualified.
		let cutoff = min(points[(points.len() as f64 * 0.33) as usize], 0);
		// We must use >= here to ensure that we don't disqualify all validators. e.g. if all the
		// validators are at -2880 reputation than the cutoff will be -2880, and all validators will
		// be disqualified.
		Reputations::<Runtime>::get(validator_id).reputation_points >= cutoff
	}
}
