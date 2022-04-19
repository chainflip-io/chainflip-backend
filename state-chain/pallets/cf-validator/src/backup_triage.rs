use crate::*;
use cf_traits::{BackupOrPassive, BackupValidators, MaxValue};
use sp_std::cmp::Reverse;

/// Tracker for backup and passive validators.
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct BackupTriage<Id, Amount> {
	pub backup: Vec<Bid<Id, Amount>>,
	pub passive: Vec<Bid<Id, Amount>>,
	backup_group_size_target: u32,
}

impl<Id, Amount> Default for BackupTriage<Id, Amount> {
	fn default() -> Self {
		BackupTriage { backup: Vec::new(), passive: Vec::new(), backup_group_size_target: 0 }
	}
}

pub type RuntimeBackupTriage<T> =
	BackupTriage<<T as Chainflip>::ValidatorId, <T as Chainflip>::Amount>;

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, PartialOrd, Ord)]
pub struct Bid<Id, Amount> {
	pub validator_id: Id,
	pub amount: Amount,
}

impl<Id, Amount> From<(Id, Amount)> for Bid<Id, Amount> {
	fn from(bid: (Id, Amount)) -> Self {
		Self { validator_id: bid.0, amount: bid.1 }
	}
}

impl<Id, Amount> BackupTriage<Id, Amount>
where
	Id: Ord,
	Amount: Ord + Copy + Default + Zero + MaxValue,
{
	pub fn new<AccountState: ChainflipAccount>(
		mut backup_candidates: Vec<(Id, Amount)>,
		backup_group_size_target: usize,
	) -> Self
	where
		Id: IsType<AccountState::AccountId>,
	{
		let mut triage_result = if backup_group_size_target > backup_candidates.len() {
			Self {
				backup: backup_candidates.into_iter().map(Into::into).collect(),
				passive: Vec::new(),
				backup_group_size_target: backup_group_size_target as u32,
			}
		} else {
			// Sort the candidates by decreasing bid.
			backup_candidates.sort_unstable_by_key(|(_, amount)| Reverse(*amount));
			let passive = backup_candidates.split_off(backup_group_size_target);
			Self {
				backup: backup_candidates.into_iter().map(Into::into).collect(),
				passive: passive.into_iter().map(Into::into).collect(),
				backup_group_size_target: backup_group_size_target as u32,
			}
		};

		triage_result.sort_backups_and_passives_by_validator_id();

		for validator_id in triage_result.backup_validators() {
			AccountState::set_backup_or_passive(validator_id.into_ref(), BackupOrPassive::Backup);
		}
		for validator_id in triage_result.passive_validators() {
			AccountState::set_backup_or_passive(validator_id.into_ref(), BackupOrPassive::Passive);
		}

		triage_result
	}

	pub fn backup_validators(&self) -> impl Iterator<Item = &Id> {
		self.backup.iter().map(|bid| &bid.validator_id)
	}

	pub fn passive_validators(&self) -> impl Iterator<Item = &Id> {
		self.passive.iter().map(|bid| &bid.validator_id)
	}

	fn lowest_backup_bid(&self) -> Amount {
		if self.backup_group_size_target == 0 {
			return Amount::MAX
		}
		self.backup.iter().map(|bid| bid.amount).min().unwrap_or_default()
	}

	fn highest_passive_bid(&self) -> Amount {
		self.passive.iter().map(|bid| bid.amount).max().unwrap_or_default()
	}

	pub fn adjust_validator<AccountState: ChainflipAccount>(
		&mut self,
		validator_id: Id,
		new_bid_amount: Amount,
	) where
		Id: IsType<AccountState::AccountId>,
	{
		if new_bid_amount.is_zero() {
			AccountState::set_backup_or_passive(validator_id.into_ref(), BackupOrPassive::Passive);
			self.kill(validator_id);
			self.resize::<AccountState>();
			return
		}

		// Cache these here before we start mutating the sets.
		let (lowest_backup_bid, highest_passive_bid) =
			(self.lowest_backup_bid(), self.highest_passive_bid());

		match (
			self.passive.binary_search_by(|bid| bid.validator_id.cmp(&validator_id)),
			self.backup.binary_search_by(|bid| bid.validator_id.cmp(&validator_id)),
		) {
			// The validator is in the passive set.
			(Ok(p), Err(b)) =>
				if new_bid_amount > lowest_backup_bid {
					// Promote the bidder to the backup set.
					let mut promoted = self.passive.remove(p);
					AccountState::set_backup_or_passive(
						promoted.validator_id.into_ref(),
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
						demoted.validator_id.into_ref(),
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
						validator_id.into_ref(),
						BackupOrPassive::Backup,
					);
					self.backup.insert(b, Bid::from((validator_id, new_bid_amount)));
				} else {
					AccountState::set_backup_or_passive(
						validator_id.into_ref(),
						BackupOrPassive::Passive,
					);
					self.passive.insert(p, Bid::from((validator_id, new_bid_amount)));
				},
			(Ok(_), Ok(_)) => unreachable!("Validator cannot be in both backup and passive"),
		}

		// We might have to resize the backup set to fit within the target size.
		self.resize::<AccountState>();
	}

	fn resize<AccountState: ChainflipAccount>(&mut self)
	where
		Id: IsType<AccountState::AccountId>,
	{
		// First, sort by bid such that we can pop the lowest backup and highest passive
		// respectively.
		self.backup.sort_unstable_by_key(|bid| Reverse(bid.amount));
		self.passive.sort_unstable_by_key(|bid| bid.amount);

		// Demote any excess backups.
		while self.backup.len() > self.backup_group_size_target as usize {
			let demoted = self.backup.pop().expect("backup set is not empty");
			AccountState::set_backup_or_passive(
				demoted.validator_id.into_ref(),
				BackupOrPassive::Passive,
			);
			self.passive.push(demoted);
		}

		// Promote any passives that fit in the backup target size.
		while self.backup.len() < self.backup_group_size_target as usize && !self.passive.is_empty()
		{
			let promoted = self.passive.pop().expect("passive set is not empty");
			AccountState::set_backup_or_passive(
				promoted.validator_id.into_ref(),
				BackupOrPassive::Backup,
			);
			self.backup.push(promoted);
		}

		// Restore the original sort order.
		self.sort_backups_and_passives_by_validator_id();
	}

	fn kill(&mut self, validator_id: Id) {
		let _ = self
			.passive
			.binary_search_by(|bid| bid.validator_id.cmp(&validator_id))
			.map(|i| {
				self.passive.remove(i);
			})
			.or_else(|_| {
				self.backup
					.binary_search_by(|bid| bid.validator_id.cmp(&validator_id))
					.map(|i| {
						self.backup.remove(i);
					})
			});
	}

	fn sort_backups_and_passives_by_validator_id(&mut self) {
		self.backup
			.sort_unstable_by(|left, right| left.validator_id.cmp(&right.validator_id));
		self.passive
			.sort_unstable_by(|left, right| left.validator_id.cmp(&right.validator_id));
	}
}

impl<T: Config> BackupValidators for Pallet<T> {
	type ValidatorId = ValidatorIdOf<T>;

	fn backup_validators() -> Vec<Self::ValidatorId> {
		BackupValidatorTriage::<T>::get().backup_validators().cloned().collect()
	}
}

#[cfg(test)]
mod test_backup_triage {
	use super::*;
	use cf_traits::mocks::chainflip_account::MockChainflipAccount;
	use sp_std::{collections::btree_set::BTreeSet, iter::FromIterator};

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
				assert!(MockChainflipAccount::is_backup(bid.validator_id.into_ref()));
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
				assert!(MockChainflipAccount::is_passive(bid.validator_id.into_ref()));
			});
		};
	}

	type TestBackupTriage = BackupTriage<u64, u32>;

	#[test]
	fn test_new() {
		const CANDIDATES: &[(u64, u32)] = &[(5, 5), (1, 1), (3, 3), (4, 4), (2, 2)];

		let triage = TestBackupTriage::new::<MockChainflipAccount>(CANDIDATES.to_vec(), 3);

		assert_eq!(triage.lowest_backup_bid(), 3, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 2, "{:?}", triage);
		check_invariants!(triage);

		let triage = TestBackupTriage::new::<MockChainflipAccount>(CANDIDATES.to_vec(), 5);
		assert_eq!(triage.lowest_backup_bid(), 1, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 0, "{:?}", triage);
		check_invariants!(triage);

		let triage = TestBackupTriage::new::<MockChainflipAccount>(CANDIDATES.to_vec(), 10);
		assert_eq!(triage.lowest_backup_bid(), 1, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 0, "{:?}", triage);
		check_invariants!(triage);

		let triage = TestBackupTriage::new::<MockChainflipAccount>(CANDIDATES.to_vec(), 0);
		assert_eq!(triage.lowest_backup_bid(), u32::MAX, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 5, "{:?}", triage);
		check_invariants!(triage);

		let triage = TestBackupTriage::new::<MockChainflipAccount>(CANDIDATES.to_vec(), 1);
		assert_eq!(triage.lowest_backup_bid(), 5, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 4, "{:?}", triage);
		check_invariants!(triage);
	}

	#[test]
	fn test_promotion_from_passive() {
		const CANDIDATES: &[(u64, u32)] = &[(5, 5), (1, 1), (3, 3), (4, 4), (2, 2)];
		const TEST_SIZE: usize = 3;

		let mut triage =
			TestBackupTriage::new::<MockChainflipAccount>(CANDIDATES.to_vec(), TEST_SIZE);

		assert_eq!(triage.lowest_backup_bid(), 3, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 2, "{:?}", triage);
		check_invariants!(triage);

		// Promote validator 2
		triage.adjust_validator::<MockChainflipAccount>(2, 6);
		assert_eq!(triage.lowest_backup_bid(), 4, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 3, "{:?}", triage);
		check_invariants!(triage);

		// Validator 3 staked up to lowest backup bid, no promotion
		triage.adjust_validator::<MockChainflipAccount>(3, 4);
		assert_eq!(triage.lowest_backup_bid(), 4, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 4, "{:?}", triage);
		check_invariants!(triage);

		// Validator 3 staked up to promotion
		triage.adjust_validator::<MockChainflipAccount>(3, 5);
		assert_eq!(triage.lowest_backup_bid(), 5, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 4, "{:?}", triage);
		check_invariants!(triage);

		assert_eq!(
			BTreeSet::from_iter(triage.backup_validators()),
			BTreeSet::from_iter(&[2, 3, 5])
		);
	}

	#[test]
	fn test_promotion_from_outside() {
		const CANDIDATES: &[(u64, u32)] = &[(5, 5), (1, 1), (3, 3)];
		const TEST_SIZE: usize = 3;

		let mut triage =
			TestBackupTriage::new::<MockChainflipAccount>(CANDIDATES.to_vec(), TEST_SIZE);
		assert_eq!(triage.lowest_backup_bid(), 1, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 0, "{:?}", triage);
		check_invariants!(triage);

		// Promote validator 4
		triage.adjust_validator::<MockChainflipAccount>(4, 4);
		assert_eq!(triage.lowest_backup_bid(), 3, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 1, "{:?}", triage);
		check_invariants!(triage);

		// Add validator 2 to passives
		triage.adjust_validator::<MockChainflipAccount>(2, 2);
		assert_eq!(triage.lowest_backup_bid(), 3, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 2, "{:?}", triage);
		check_invariants!(triage);
	}

	#[test]
	fn test_demotion() {
		const CANDIDATES: &[(u64, u32)] = &[(5, 5), (1, 1), (3, 3)];
		const TEST_SIZE: usize = 3;

		let mut triage =
			TestBackupTriage::new::<MockChainflipAccount>(CANDIDATES.to_vec(), TEST_SIZE);
		// Initial: [5, 3, 1] / []
		assert_eq!(triage.lowest_backup_bid(), 1, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 0, "{:?}", triage);
		check_invariants!(triage);

		// Promote validator 4: [5, 4, 3] / [1]
		triage.adjust_validator::<MockChainflipAccount>(4, 4);
		assert_eq!(triage.lowest_backup_bid(), 3, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 1, "{:?}", triage);
		check_invariants!(triage);

		// Add validator 2 to passives: [5, 4, 3] / [2, 1]
		triage.adjust_validator::<MockChainflipAccount>(2, 2);
		assert_eq!(triage.lowest_backup_bid(), 3, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 2, "{:?}", triage);
		check_invariants!(triage);

		// Validator 5 unstakes entirely: [4, 3, 2] / [1]
		triage.adjust_validator::<MockChainflipAccount>(5, 0);
		assert_eq!(triage.lowest_backup_bid(), 2, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 1, "{:?}", triage);
		check_invariants!(triage);
		assert!(triage.backup_validators().count() + triage.passive_validators().count() == 4);

		// Validator 1 unstakes entirely: [4, 3, 2] / []
		triage.adjust_validator::<MockChainflipAccount>(1, 0);
		assert_eq!(triage.lowest_backup_bid(), 2, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 0, "{:?}", triage);
		check_invariants!(triage);
		assert!(triage.backup_validators().count() + triage.passive_validators().count() == 3);

		// Validator 2 unstakes entirely: [4, 3] / []
		triage.adjust_validator::<MockChainflipAccount>(2, 0);
		assert_eq!(triage.lowest_backup_bid(), 3, "{:?}", triage);
		assert_eq!(triage.highest_passive_bid(), 0, "{:?}", triage);
		check_invariants!(triage);
		assert!(triage.backup_validators().count() + triage.passive_validators().count() == 2);
	}
}
