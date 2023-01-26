use crate::*;
use cf_traits::{AuctionOutcome, Bid};
use sp_runtime::traits::AtLeast32BitUnsigned;
use sp_std::collections::btree_set::BTreeSet;

pub(crate) const SECONDARY_CANDIDATE_FRACTION: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, Default)]
pub struct RotationState<Id, Amount> {
	pub primary_candidates: Vec<Id>,
	pub secondary_candidates: Vec<Id>,
	pub banned: BTreeSet<Id>,
	pub bond: Amount,
}

impl<Id: Ord + Clone, Amount: AtLeast32BitUnsigned + Copy> RotationState<Id, Amount> {
	pub fn from_auction_outcome<T>(
		AuctionOutcome { winners, losers, bond }: AuctionOutcome<Id, Amount>,
	) -> Self
	where
		T: Config<Amount = Amount> + Chainflip<ValidatorId = Id>,
	{
		debug_assert!(losers.is_sorted_by_key(|&Bid { amount, .. }| Reverse(amount)));
		let authorities = Pallet::<T>::current_authorities().into_iter().collect::<BTreeSet<_>>();

		// We don't want backups with too low a stake to be in the authority set.
		let highest_staked_qualified_backup_nodes =
			Pallet::<T>::highest_staked_qualified_backup_nodes_lookup();

		RotationState {
			primary_candidates: winners,
			secondary_candidates: losers
				.into_iter()
				// We only allow current authorities or backup validators to be secondary
				// candidates.
				.filter_map(|Bid { bidder_id, .. }| {
					if highest_staked_qualified_backup_nodes.contains(&bidder_id) ||
						authorities.contains(&bidder_id)
					{
						Some(bidder_id)
					} else {
						None
					}
				})
				// Limit the number of secondary candidates according to the size of the
				// backup_percentage and the fracction of that, which can be secondary candidates
				.take(Pallet::<T>::backup_reward_nodes_limit() / SECONDARY_CANDIDATE_FRACTION)
				.collect(),
			banned: Default::default(),
			bond,
		}
	}

	pub fn ban(&mut self, new_banned: BTreeSet<Id>) {
		for id in new_banned {
			self.banned.insert(id);
		}
	}

	pub fn authority_candidates<I: FromIterator<Id>>(&self) -> I {
		self.primary_candidates
			.iter()
			.chain(&self.secondary_candidates)
			.filter(|id| !self.banned.contains(id))
			.take(self.primary_candidates.len())
			.cloned()
			.collect::<I>()
	}

	pub fn num_primary_candidates(&self) -> u32 {
		self.primary_candidates.len() as u32
	}
}

#[cfg(test)]
mod rotation_state_tests {
	use super::*;

	#[test]
	fn banning_is_additive() {
		let mut rotation_state = RotationState::<u64, u64> {
			primary_candidates: (0..10).collect(),
			secondary_candidates: (20..30).collect(),
			banned: Default::default(),
			bond: 500,
		};

		let first_ban = BTreeSet::from([8, 9, 7]);
		rotation_state.ban(first_ban.clone());
		assert_eq!(first_ban, rotation_state.banned);

		let second_ban = BTreeSet::from([1, 2, 3]);
		rotation_state.ban(second_ban.clone());
		assert_eq!(
			first_ban.union(&second_ban).into_iter().cloned().collect::<BTreeSet<_>>(),
			rotation_state.banned
		);
	}

	#[test]
	fn authority_candidates_prefers_primaries_and_excludes_banned() {
		let rotation_state = RotationState::<u64, u64> {
			primary_candidates: (0..10).collect(),
			secondary_candidates: (20..30).collect(),
			banned: BTreeSet::from([1, 2, 4]),
			bond: 500,
		};

		let candidates: Vec<_> = rotation_state.authority_candidates();

		assert_eq!(candidates, vec![0, 3, 5, 6, 7, 8, 9, 20, 21, 22]);
	}
}
