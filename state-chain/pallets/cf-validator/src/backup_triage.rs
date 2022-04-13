use crate::*;
use cf_traits::{BackupOrPassive, BackupValidators};
use sp_std::cmp::Reverse;

/// Tracker for backup and passive validators.
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct BackupTriageResult<T: Chainflip> {
	pub backup: Vec<(T::ValidatorId, T::Amount)>,
	pub passive: Vec<(T::ValidatorId, T::Amount)>,
	backup_group_size_target: u32,
}

impl<T: Chainflip> Default for BackupTriageResult<T> {
	fn default() -> Self {
		Self { backup: vec![], passive: vec![], backup_group_size_target: 0 }
	}
}

impl<T: Chainflip> BackupTriageResult<T> {
	pub fn new(
		mut backup_candidates: Vec<(T::ValidatorId, T::Amount)>,
		backup_group_size_target: usize,
	) -> Self {
		// Sort the candidates by decreasing bid.
		backup_candidates.sort_unstable_by(|(_, a), (_, b)| b.cmp(a));

		let mut triage_result = if backup_group_size_target > backup_candidates.len() {
			Self {
				backup: backup_candidates,
				backup_group_size_target: backup_group_size_target as u32,
				..Default::default()
			}
		} else {
			let (backup, passive) = backup_candidates.split_at(backup_group_size_target);
			Self {
				backup: backup.to_vec(),
				passive: passive.to_vec(),
				backup_group_size_target: backup_group_size_target as u32,
			}
		};

		triage_result.sort_all();
		triage_result
	}

	pub fn adjust_validator<AccountState: ChainflipAccount>(
		&mut self,
		validator_id: T::ValidatorId,
		new_total_stake: T::Amount,
	) where
		ValidatorIdOf<T>: IsType<AccountState::AccountId>,
	{
		self.update_stake_for_bidder::<AccountState>(validator_id, new_total_stake);

		// Sort and, if necessary, resize the backup set.
		self.sort_all();
		while self.backup.len() > self.backup_group_size_target as usize {
			let demoted_backup = self.backup.pop().unwrap();
			AccountState::set_backup_or_passive(
				demoted_backup.0.into_ref(),
				BackupOrPassive::Passive,
			);
			self.passive.push(demoted_backup);
		}
		while self.backup.len() < self.backup_group_size_target as usize {
			let promoted_passive = self.passive.pop().unwrap();
			AccountState::set_backup_or_passive(
				promoted_passive.0.into_ref(),
				BackupOrPassive::Backup,
			);
			self.backup.push(promoted_passive);
		}
	}

	/// Note: relies on the backup bids remaining sorted by increasing bid.
	fn lowest_backup_bid(&self) -> T::Amount {
		self.backup.first().map(|(_, amount)| *amount).unwrap_or(Zero::zero())
	}

	/// Note: relies on the passive bids remaining sorted by decreasing bid.
	fn highest_passive_bid(&self) -> T::Amount {
		self.passive.first().map(|(_, amount)| *amount).unwrap_or(Zero::zero())
	}

	/// Sort backups by increasing bid and passives by decreasing bid.
	fn sort_all(&mut self) {
		self.backup.sort_unstable_by_key(|(_, bid)| *bid);
		self.passive.sort_unstable_by_key(|(_, bid)| Reverse(*bid));
	}

	fn update_stake_for_bidder<AccountState: ChainflipAccount>(
		&mut self,
		validator_id: T::ValidatorId,
		amount: T::Amount,
	) where
		ValidatorIdOf<T>: IsType<AccountState::AccountId>,
	{
		// Cache these here before we start mutating the sets.
		let (lowest_backup_bid, highest_passive_bid) =
			(self.lowest_backup_bid(), self.highest_passive_bid());

		// Required for the binary search.
		self.backup.sort_unstable_by(|(left, _), (right, _)| left.cmp(&right));
		self.passive.sort_unstable_by(|(left, _), (right, _)| left.cmp(&right));

		match (
			self.passive.binary_search_by(|(id, _)| id.cmp(&validator_id)),
			self.backup.binary_search_by(|(id, _)| id.cmp(&validator_id)),
		) {
			// The validator is in the passive set.
			(Ok(p), Err(b)) =>
				if amount > lowest_backup_bid {
					self.passive.remove(p);
					AccountState::set_backup_or_passive(
						validator_id.into_ref(),
						BackupOrPassive::Backup,
					);
					self.backup.insert(b, (validator_id, amount));
				} else {
					self.passive[p].1 = amount;
				},
			// The validator is in the backup set.
			(Err(p), Ok(b)) =>
				if amount < highest_passive_bid {
					self.backup.remove(b);
					AccountState::set_backup_or_passive(
						validator_id.into_ref(),
						BackupOrPassive::Passive,
					);
					self.passive.insert(p, (validator_id, amount));
				} else {
					self.backup[b].1 = amount;
				},
			// The validator is in neither the passive nor backup set.
			(Err(p), Err(b)) =>
				if amount > lowest_backup_bid {
					AccountState::set_backup_or_passive(
						validator_id.into_ref(),
						BackupOrPassive::Backup,
					);
					self.backup.insert(b, (validator_id, amount));
				} else {
					AccountState::set_backup_or_passive(
						validator_id.into_ref(),
						BackupOrPassive::Passive,
					);
					self.passive.insert(p, (validator_id, amount));
				},
			(Ok(_), Ok(_)) => unreachable!("Validator cannot be in both backup and passive"),
		}
	}
}

impl<T: Config> BackupValidators for Pallet<T> {
	type ValidatorId = ValidatorIdOf<T>;

	fn backup_validators() -> Vec<Self::ValidatorId> {
		BackupValidatorTriage::<T>::get()
			.backup
			.into_iter()
			.map(|(validator_id, _)| validator_id)
			.collect()
	}
}
