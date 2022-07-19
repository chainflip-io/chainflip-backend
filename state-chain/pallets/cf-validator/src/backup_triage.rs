use crate::*;
use cf_traits::{BackupNodes, Bid};
use sp_runtime::traits::AtLeast32BitUnsigned;
use sp_std::cmp::Reverse;

/// Tracker for backup nodes
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
pub struct BackupTriage<Id, Amount> {
	/// Sorted by id
	backup: Vec<Bid<Id, Amount>>,
}

impl<Id, Amount> Default for BackupTriage<Id, Amount> {
	fn default() -> Self {
		BackupTriage { backup: Vec::new() }
	}
}

pub type RuntimeBackupTriage<T> =
	BackupTriage<<T as Chainflip>::ValidatorId, <T as Chainflip>::Amount>;

impl<Id, Amount> BackupTriage<Id, Amount>
where
	Id: Ord,
	Amount: AtLeast32BitUnsigned + Copy,
{
	pub fn new<AccountState: ChainflipAccount>(mut backup_candidates: Vec<Bid<Id, Amount>>) -> Self
	where
		Id: IsType<AccountState::AccountId>,
	{
		// Sort by validator id
		backup_candidates.sort_unstable_by(|left, right| left.bidder_id.cmp(&right.bidder_id));
		Self { backup: backup_candidates }
	}

	// TODO: Inline this?? Or is it worth keeping to test it?
	pub fn adjust_bid<AccountState: ChainflipAccount>(
		&mut self,
		bidder_id: Id,
		new_bid_amount: Amount,
	) where
		Id: IsType<AccountState::AccountId>,
	{
		if new_bid_amount.is_zero() {
			// Delete them from the backup set
			self.backup
				.binary_search_by(|bid| bid.bidder_id.cmp(&bidder_id))
				.map(|i| {
					self.backup.remove(i);
				})
				.expect("They should be in the backup list if we are adjusting their bid");
		} else {
			match self.backup.binary_search_by(|bid| bid.bidder_id.cmp(&bidder_id)) {
				Ok(index) => {
					self.backup[index].amount = new_bid_amount;
				},
				Err(index) => {
					self.backup.insert(index, Bid { bidder_id, amount: new_bid_amount });
				},
			};
		}
	}
}

impl<T: Config> BackupNodes for Pallet<T> {
	type ValidatorId = ValidatorIdOf<T>;

	fn n_backup_nodes() -> usize {
		Percent::from_percent(BackupNodePercentage::<T>::get()) *
			Self::current_authority_count() as usize
	}

	fn highest_staked_backup_nodes(n: usize) -> Vec<Self::ValidatorId> {
		let mut backups = BackupValidatorTriage::<T>::get().backup;
		backups.sort_unstable_by_key(|Bid { amount, .. }| Reverse(*amount));
		backups.into_iter().take(n).map(|bid| bid.bidder_id).collect()
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
