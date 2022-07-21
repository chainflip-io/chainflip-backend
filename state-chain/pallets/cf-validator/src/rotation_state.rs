use crate::*;
use cf_traits::{AuctionOutcome, Bid};
use sp_runtime::traits::AtLeast32BitUnsigned;
use sp_std::collections::btree_set::BTreeSet;

pub(crate) const SECONDARY_CANDIDATE_FRACTION: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, Default)]
pub struct RotationState<Id, Amount> {
	primary_candidates: Vec<Id>,
	secondary_candidates: Vec<Id>,
	banned: BTreeSet<Id>,
	pub bond: Amount,
}

impl<Id: Ord + Clone, Amount: AtLeast32BitUnsigned + Copy> RotationState<Id, Amount> {
	pub fn from_auction_outcome<T>(
		AuctionOutcome { winners, losers, bond }: AuctionOutcome<Id, Amount>,
	) -> Self
	where
		T: Config<Amount = Amount> + Chainflip<ValidatorId = Id>,
	{
		let authorities = Pallet::<T>::current_authorities().into_iter().collect::<BTreeSet<_>>();

		let highest_staked_qualified_backup_nodes = Pallet::<T>::highest_staked_qualified_backup_nodes();

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

	pub fn ban(&mut self, new_banned: impl IntoIterator<Item = Id>) {
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
