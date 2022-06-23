use crate::*;
use cf_traits::{AuctionOutcome, BackupNodes, Bid};
use sp_runtime::traits::AtLeast32BitUnsigned;
use sp_std::collections::btree_set::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, Default)]
pub struct RotationStatus<Id, Amount> {
	primary_candidates: Vec<Id>,
	backup_candidates: Vec<Id>,
	auction_losers: Vec<Bid<Id, Amount>>,
	banned: Vec<Id>,
	pub bond: Amount,
	target_set_size: u8,
}

impl<Id: Ord + Clone, Amount: AtLeast32BitUnsigned + Copy> RotationStatus<Id, Amount> {
	pub fn new(
		auction_winners: Vec<Id>,
		auction_losers: Vec<Bid<Id, Amount>>,
		bond: Amount,
		mut backup_validators: Vec<Id>,
	) -> Self {
		let target_set_size = auction_winners.len() as u8;
		let auction_losers_lookup =
			BTreeSet::from_iter(auction_losers.iter().map(|bid| &bid.bidder_id));
		backup_validators.retain(|id| auction_losers_lookup.contains(id));
		Self {
			primary_candidates: auction_winners,
			backup_candidates: backup_validators,
			auction_losers,
			banned: Vec::new(),
			bond,
			target_set_size,
		}
	}

	pub fn from_auction_outcome<B: BackupNodes<ValidatorId = Id>>(
		auction_outcome: AuctionOutcome<Id, Amount>,
	) -> Self {
		Self::new(
			auction_outcome.winners,
			auction_outcome.losers,
			auction_outcome.bond,
			B::backup_nodes(),
		)
	}

	pub fn ban(&mut self, mut banned: Vec<Id>) {
		self.banned.append(&mut banned);
	}

	fn authority_candidates_iter(&self) -> impl Iterator<Item = Id> + '_ {
		self.primary_candidates
			.iter()
			.chain(&self.backup_candidates)
			.filter(|id| !self.banned.contains(id))
			.take(self.target_set_size as usize)
			.cloned()
	}

	pub fn authority_candidates(&self) -> Vec<Id> {
		self.authority_candidates_iter().collect()
	}

	pub fn to_backup_triage<AccountState: ChainflipAccount>(
		self,
		backup_group_size_target: usize,
	) -> BackupTriage<Id, Amount>
	where
		Id: IsType<AccountState::AccountId>,
	{
		let authorities_lookup = BTreeSet::from_iter(self.authority_candidates());
		let new_backup_candidates = self
			.auction_losers
			.into_iter()
			.filter(|bid| !authorities_lookup.contains(&bid.bidder_id))
			.collect();
		BackupTriage::new::<AccountState>(new_backup_candidates, backup_group_size_target)
	}

	pub fn reset(&mut self) {
		self.banned.clear();
	}
}
