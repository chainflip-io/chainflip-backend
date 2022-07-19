use crate::*;
use cf_traits::{AuctionOutcome, BackupNodes, Bid};
use sp_runtime::traits::AtLeast32BitUnsigned;
use sp_std::collections::btree_set::BTreeSet;

pub(crate) const SECONDARY_CANDIDATE_FRACTION: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, Default)]
pub struct RotationState<Id, Amount> {
	primary_candidates: Vec<Id>,
	secondary_candidates: Vec<Id>,
	banned: BTreeSet<Id>,
	pub bond: Amount,
	target_set_size: u8,
}

impl<Id: Ord + Clone, Amount: AtLeast32BitUnsigned + Copy> RotationState<Id, Amount> {
	pub fn new(primary_candidates: Vec<Id>, secondary_candidates: Vec<Id>, bond: Amount) -> Self {
		let target_set_size = primary_candidates.len() as u8;
		Self {
			primary_candidates,
			secondary_candidates,
			banned: Default::default(),
			bond,
			target_set_size,
		}
	}

	// We need to work out how many to take for the secondary candidate fraction
	pub fn from_auction_outcome<T>(auction_outcome: AuctionOutcome<Id, Amount>) -> Self
	where
		T: Config<Amount = Amount> + Chainflip<ValidatorId = Id>,
	{
		let authorities = Pallet::<T>::current_authorities().into_iter().collect::<BTreeSet<_>>();

		let backup_node_percentage = BackupNodePercentage::<T>::get();

		let num_backup_nodes = (authorities.len() * backup_node_percentage as usize) / 100;

		// Limit the number of secondary candidates according to the size of the backup_percentage
		// and the fracction of that, which can be secondary candidates
		let max_secondary_candidates = num_backup_nodes / (SECONDARY_CANDIDATE_FRACTION);

		// only the highest staked nodes are eligible
		let eligible_backups = Pallet::<T>::highest_staked_backup_nodes(num_backup_nodes);

		Self::new(
			auction_outcome.winners,
			auction_outcome
				.losers
				.into_iter()
				// We only allow current authorities or backup validators to be secondary
				// candidates.
				.filter_map(|Bid { bidder_id, .. }| {
					if eligible_backups.contains(&bidder_id) || authorities.contains(&bidder_id) {
						Some(bidder_id)
					} else {
						None
					}
				})
				.take(max_secondary_candidates)
				.collect(),
			auction_outcome.bond,
		)
	}

	pub fn ban(&mut self, new_banned: impl IntoIterator<Item = Id>) {
		for id in new_banned {
			self.banned.insert(id);
		}
	}

	pub fn authority_candidates_iter(&self) -> impl Iterator<Item = &Id> {
		self.primary_candidates
			.iter()
			.chain(&self.secondary_candidates)
			.filter(|id| !self.banned.contains(id))
			.take(self.target_set_size as usize)
	}

	pub fn authority_candidates<I: FromIterator<Id>>(&self) -> I {
		self.authority_candidates_iter().cloned().collect::<I>()
	}

	pub fn num_primary_candidates(&self) -> u32 {
		self.primary_candidates.len() as u32
	}
}
