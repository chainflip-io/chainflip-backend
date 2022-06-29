use crate::*;
use cf_traits::{BackupNodes, BackupOrPassive, Bid};
use sp_runtime::traits::AtLeast32BitUnsigned;
use sp_std::cmp::Reverse;

/// Tracker for backup and passive validators.
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
pub struct BackupTriage<Id, Amount> {
	backup: Vec<Bid<Id, Amount>>,
	passive: Vec<Bid<Id, Amount>>,
	backup_group_size_target: u32,
}

impl<Id, Amount> Default for BackupTriage<Id, Amount> {
	fn default() -> Self {
		BackupTriage { backup: Vec::new(), passive: Vec::new(), backup_group_size_target: 0 }
	}
}

pub type RuntimeBackupTriage<T> =
	BackupTriage<<T as Chainflip>::ValidatorId, <T as Chainflip>::Amount>;

impl<Id, Amount> BackupTriage<Id, Amount>
where
	Id: Ord,
	Amount: AtLeast32BitUnsigned + Copy,
{
	pub fn new<AccountState: ChainflipAccount>(
		mut backup_candidates: Vec<Bid<Id, Amount>>,
		backup_group_size_target: usize,
	) -> Self
	where
		Id: IsType<AccountState::AccountId>,
	{
		let mut triage_result = if backup_group_size_target > backup_candidates.len() {
			Self {
				backup: backup_candidates,
				passive: Vec::new(),
				backup_group_size_target: backup_group_size_target as u32,
			}
		} else {
			// Sort the candidates by decreasing bid.
			backup_candidates.sort_unstable_by_key(|Bid { amount, .. }| Reverse(*amount));
			let passive = backup_candidates.split_off(backup_group_size_target);
			Self {
				backup: backup_candidates,
				passive,
				backup_group_size_target: backup_group_size_target as u32,
			}
		};

		triage_result.sort_all_by_validator_id();

		// TODO:
		// It's not this simple. For example, there might be old backup validators who are no longer
		// in either of these sets because they were banned from the auction.

		for validator_id in triage_result.backup_nodes() {
			AccountState::set_backup_or_passive(validator_id.into_ref(), BackupOrPassive::Backup);
		}
		for validator_id in triage_result.passive_nodes() {
			AccountState::set_backup_or_passive(validator_id.into_ref(), BackupOrPassive::Passive);
		}

		triage_result
	}

	pub fn backup_nodes(&self) -> impl Iterator<Item = &Id> {
		self.backup.iter().map(|bid| &bid.bidder_id)
	}

	pub fn passive_nodes(&self) -> impl Iterator<Item = &Id> {
		self.passive.iter().map(|bid| &bid.bidder_id)
	}

	fn lowest_backup_bid(&self) -> Amount {
		if self.backup_group_size_target == 0 {
			return Amount::max_value()
		}
		self.backup.iter().map(|bid| bid.amount).min().unwrap_or_else(Zero::zero)
	}

	fn highest_passive_bid(&self) -> Amount {
		self.passive.iter().map(|bid| bid.amount).max().unwrap_or_else(Zero::zero)
	}

	pub fn adjust_bid<AccountState: ChainflipAccount>(
		&mut self,
		bidder_id: Id,
		new_bid_amount: Amount,
	) where
		Id: IsType<AccountState::AccountId>,
	{
		if new_bid_amount.is_zero() {
			let find_and_remove_bidder_from = |bids: &mut Vec<Bid<Id, Amount>>| {
				bids.binary_search_by(|bid| bid.bidder_id.cmp(&bidder_id)).map(|i| {
					bids.remove(i);
				})
			};

			let _ = find_and_remove_bidder_from(&mut self.passive)
				.or_else(|_| find_and_remove_bidder_from(&mut self.backup));

			AccountState::set_backup_or_passive(bidder_id.into_ref(), BackupOrPassive::Passive);
		} else {
			// Cache these here before we start mutating the sets.
			let (lowest_backup_bid, highest_passive_bid) =
				(self.lowest_backup_bid(), self.highest_passive_bid());

			match (
				self.passive.binary_search_by(|bid| bid.bidder_id.cmp(&bidder_id)),
				self.backup.binary_search_by(|bid| bid.bidder_id.cmp(&bidder_id)),
			) {
				// The validator is in the passive set.
				(Ok(p), Err(b)) =>
					if new_bid_amount > lowest_backup_bid {
						// Promote the bidder to the backup set.
						let mut promoted = self.passive.remove(p);
						AccountState::set_backup_or_passive(
							promoted.bidder_id.into_ref(),
							BackupOrPassive::Backup,
						);
						promoted.amount = new_bid_amount;
						self.backup.insert(b, promoted);
					} else {
						// No change, just update the bid.
						self.passive[p].amount = new_bid_amount;
					},
				// The validator is in the backup set.
				(Err(p), Ok(b)) =>
					if new_bid_amount < highest_passive_bid {
						// Demote the bidder to the passive set.
						let mut demoted = self.backup.remove(b);
						AccountState::set_backup_or_passive(
							demoted.bidder_id.into_ref(),
							BackupOrPassive::Passive,
						);
						demoted.amount = new_bid_amount;
						self.passive.insert(p, demoted);
					} else {
						self.backup[b].amount = new_bid_amount;
					},
				// The validator is in neither the passive nor backup set.
				(Err(p), Err(b)) =>
					if new_bid_amount > lowest_backup_bid {
						AccountState::set_backup_or_passive(
							bidder_id.into_ref(),
							BackupOrPassive::Backup,
						);
						self.backup.insert(b, Bid::from((bidder_id, new_bid_amount)));
					} else {
						AccountState::set_backup_or_passive(
							bidder_id.into_ref(),
							BackupOrPassive::Passive,
						);
						self.passive.insert(p, Bid::from((bidder_id, new_bid_amount)));
					},
				(Ok(_), Ok(_)) => unreachable!("Validator cannot be in both backup and passive"),
			}
		}

		// We might have to resize the backup set to fit within the target size.
		if self.backup.len() != self.backup_group_size_target as usize {
			// First, sort by bid such that we can pop the lowest backup and highest passive
			// respectively.
			self.backup.sort_unstable_by_key(|bid| Reverse(bid.amount));
			self.passive.sort_unstable_by_key(|bid| bid.amount);

			// Demote any excess backups.
			while self.backup.len() > self.backup_group_size_target as usize {
				let demoted = self.backup.pop().expect("backup set is not empty");
				AccountState::set_backup_or_passive(
					demoted.bidder_id.into_ref(),
					BackupOrPassive::Passive,
				);
				self.passive.push(demoted);
			}

			// Promote any passives that fit in the backup target size.
			while self.backup.len() < self.backup_group_size_target as usize &&
				!self.passive.is_empty()
			{
				let promoted = self.passive.pop().expect("passive set is not empty");
				AccountState::set_backup_or_passive(
					promoted.bidder_id.into_ref(),
					BackupOrPassive::Backup,
				);
				self.backup.push(promoted);
			}

			// Restore the original sort order.
			self.sort_all_by_validator_id();
		}
	}

	fn sort_all_by_validator_id(&mut self) {
		let sort = |bids: &mut [Bid<Id, Amount>]| {
			bids.sort_unstable_by(|left, right| left.bidder_id.cmp(&right.bidder_id))
		};
		sort(&mut self.backup);
		sort(&mut self.passive);
	}
}

impl<T: Config> BackupNodes for Pallet<T> {
	type ValidatorId = ValidatorIdOf<T>;

	fn backup_nodes() -> Vec<Self::ValidatorId> {
		BackupValidatorTriage::<T>::get().backup_nodes().cloned().collect()
	}
}

#[cfg(test)]
mod test_backup_triage {
	use super::*;
	use crate::mock::ValidatorId;
	use cf_traits::mocks::chainflip_account::MockChainflipAccount;
	use sp_std::collections::btree_set::BTreeSet;

	fn candidates_to_bids(candidates: Vec<(ValidatorId, u32)>) -> Vec<Bid<ValidatorId, u32>> {
		candidates.into_iter().map(Into::into).collect()
	}

	macro_rules! check_invariants {
		($triage:ident) => {
			let triage_cloned = $triage.clone();
			let lowest_backup_bid = triage_cloned.lowest_backup_bid();
			let highest_passive_bid = triage_cloned.highest_passive_bid();
			let backup_set = triage_cloned.backup.into_iter().collect::<BTreeSet<_>>();
			let passive_set = triage_cloned.passive.into_iter().collect::<BTreeSet<_>>();

			assert!(
				backup_set.len() <= triage_cloned.backup_group_size_target as usize,
				"backup set should be within the target size"
			);
			assert!(
				backup_set.is_disjoint(&passive_set),
				"backup should not overlap with passive set"
			);
			assert!(
				highest_passive_bid <= lowest_backup_bid,
				"highest passive bid should be less than or equal to lowest backup bid"
			);

			backup_set.iter().for_each(|bid| {
				assert!(
					bid.amount >= lowest_backup_bid,
					"backup bids should be >= lowest backup bid"
				);
				assert!(
					bid.amount >= highest_passive_bid,
					"backup bids should be >= highest passive bid"
				);
				assert!(MockChainflipAccount::is_backup(bid.bidder_id.into_ref()));
			});
			passive_set.iter().for_each(|bid| {
				assert!(
					bid.amount <= lowest_backup_bid,
					"passive bids should be <= lowest backup bid"
				);
				assert!(
					bid.amount <= highest_passive_bid,
					"passive bids should be <= highest passive bid"
				);
				assert!(MockChainflipAccount::is_passive(bid.bidder_id.into_ref()));
			});
		};
	}

	type TestBackupTriage = BackupTriage<u64, u32>;

	#[test]
	fn test_new() {
		const CANDIDATES: &[(u64, u32)] = &[(5, 5), (1, 1), (3, 3), (4, 4), (2, 2)];
		let candidates = candidates_to_bids(CANDIDATES.to_vec());

		let triage = TestBackupTriage::new::<MockChainflipAccount>(candidates.clone(), 3);

		assert_eq!(triage.lowest_backup_bid(), 3, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 2, "{:?}", triage);
		check_invariants!(triage);

		let triage = TestBackupTriage::new::<MockChainflipAccount>(candidates.clone(), 5);
		assert_eq!(triage.lowest_backup_bid(), 1, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 0, "{:?}", triage);
		check_invariants!(triage);

		let triage = TestBackupTriage::new::<MockChainflipAccount>(candidates.clone(), 10);
		assert_eq!(triage.lowest_backup_bid(), 1, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 0, "{:?}", triage);
		check_invariants!(triage);

		let triage = TestBackupTriage::new::<MockChainflipAccount>(candidates.clone(), 0);
		assert_eq!(triage.lowest_backup_bid(), u32::MAX, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 5, "{:?}", triage);
		check_invariants!(triage);

		let triage = TestBackupTriage::new::<MockChainflipAccount>(candidates, 1);
		assert_eq!(triage.lowest_backup_bid(), 5, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 4, "{:?}", triage);
		check_invariants!(triage);
	}

	#[test]
	fn test_promotion_from_passive() {
		const CANDIDATES: &[(u64, u32)] = &[(5, 5), (1, 1), (3, 3), (4, 4), (2, 2)];
		let candidates = candidates_to_bids(CANDIDATES.to_vec());
		const TEST_SIZE: usize = 3;

		let mut triage = TestBackupTriage::new::<MockChainflipAccount>(candidates, TEST_SIZE);

		assert_eq!(triage.lowest_backup_bid(), 3, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 2, "{:?}", triage);
		check_invariants!(triage);

		// Promote node 2
		triage.adjust_bid::<MockChainflipAccount>(2, 6);
		assert_eq!(triage.lowest_backup_bid(), 4, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 3, "{:?}", triage);
		check_invariants!(triage);

		// Node 3 staked up to lowest backup bid, no promotion
		triage.adjust_bid::<MockChainflipAccount>(3, 4);
		assert_eq!(triage.lowest_backup_bid(), 4, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 4, "{:?}", triage);
		check_invariants!(triage);

		// Node 3 staked up to promotion
		triage.adjust_bid::<MockChainflipAccount>(3, 5);
		assert_eq!(triage.lowest_backup_bid(), 5, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 4, "{:?}", triage);
		check_invariants!(triage);

		assert_eq!(BTreeSet::from_iter(triage.backup_nodes()), BTreeSet::from_iter(&[2, 3, 5]));
	}

	#[test]
	fn test_promotion_from_outside() {
		const CANDIDATES: &[(u64, u32)] = &[(5, 5), (1, 1), (3, 3)];
		let candidates = candidates_to_bids(CANDIDATES.to_vec());
		const TEST_SIZE: usize = 3;

		let mut triage = TestBackupTriage::new::<MockChainflipAccount>(candidates, TEST_SIZE);
		assert_eq!(triage.lowest_backup_bid(), 1, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 0, "{:?}", triage);
		check_invariants!(triage);

		// Promote node 4
		triage.adjust_bid::<MockChainflipAccount>(4, 4);
		assert_eq!(triage.lowest_backup_bid(), 3, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 1, "{:?}", triage);
		check_invariants!(triage);

		// Add node 2 to passives
		triage.adjust_bid::<MockChainflipAccount>(2, 2);
		assert_eq!(triage.lowest_backup_bid(), 3, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 2, "{:?}", triage);
		check_invariants!(triage);
	}

	#[test]
	fn test_demotion() {
		const CANDIDATES: &[(u64, u32)] = &[(5, 5), (1, 1), (3, 3)];
		let candidates = candidates_to_bids(CANDIDATES.to_vec());
		const TEST_SIZE: usize = 3;

		let mut triage = TestBackupTriage::new::<MockChainflipAccount>(candidates, TEST_SIZE);
		// Initial: [5, 3, 1] / []
		assert_eq!(triage.lowest_backup_bid(), 1, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 0, "{:?}", triage);
		check_invariants!(triage);

		// Promote node 4: [5, 4, 3] / [1]
		triage.adjust_bid::<MockChainflipAccount>(4, 4);
		assert_eq!(triage.lowest_backup_bid(), 3, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 1, "{:?}", triage);
		check_invariants!(triage);

		// Add node 2 to passives: [5, 4, 3] / [2, 1]
		triage.adjust_bid::<MockChainflipAccount>(2, 2);
		assert_eq!(triage.lowest_backup_bid(), 3, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 2, "{:?}", triage);
		check_invariants!(triage);

		// Node 5 unstakes entirely: [4, 3, 2] / [1]
		triage.adjust_bid::<MockChainflipAccount>(5, 0);
		assert_eq!(triage.lowest_backup_bid(), 2, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 1, "{:?}", triage);
		check_invariants!(triage);
		assert!(triage.backup_nodes().count() + triage.passive_nodes().count() == 4);

		// Node 1 unstakes entirely: [4, 3, 2] / []
		triage.adjust_bid::<MockChainflipAccount>(1, 0);
		assert_eq!(triage.lowest_backup_bid(), 2, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 0, "{:?}", triage);
		check_invariants!(triage);
		assert!(triage.backup_nodes().count() + triage.passive_nodes().count() == 3);

		// Node 2 unstakes entirely: [4, 3] / []
		triage.adjust_bid::<MockChainflipAccount>(2, 0);
		assert_eq!(triage.lowest_backup_bid(), 3, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 0, "{:?}", triage);
		check_invariants!(triage);
		assert!(triage.backup_nodes().count() + triage.passive_nodes().count() == 2);
	}
}
