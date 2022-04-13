use crate::{BackupOrPassive, ChainflipAccount, ChainflipAccountData, ChainflipAccountState};
use std::{cell::RefCell, collections::HashMap};
thread_local! {
	pub static CHAINFLIP_ACCOUNTS: RefCell<HashMap<u64, ChainflipAccountData>> = RefCell::new(HashMap::new());
}

pub struct MockChainflipAccount;

impl ChainflipAccount for MockChainflipAccount {
	type AccountId = u64;

	fn get(account_id: &Self::AccountId) -> ChainflipAccountData {
		CHAINFLIP_ACCOUNTS.with(|cell| *cell.borrow().get(account_id).unwrap())
	}

	fn set_backup_or_passive(account_id: &Self::AccountId, backup_or_passive: BackupOrPassive) {
		CHAINFLIP_ACCOUNTS.with(|cell| {
			let mut map = cell.borrow_mut();
			match map.get_mut(account_id) {
				None => {
					map.insert(
						*account_id,
						ChainflipAccountData {
							state: ChainflipAccountState::BackupOrPassive(backup_or_passive),
						},
					);
				},
				Some(item) => {
					(*item).state = match item.state {
						ChainflipAccountState::CurrentAuthority => {
							panic!("Cannot set backup_or_passive on current_authority");
						},
						ChainflipAccountState::HistoricalAuthority(_) =>
							ChainflipAccountState::HistoricalAuthority(backup_or_passive),
						ChainflipAccountState::BackupOrPassive(_) =>
							ChainflipAccountState::BackupOrPassive(backup_or_passive),
					};
				},
			}
		});
	}

	fn update_validator_account_data(account_id: &Self::AccountId) {
		CHAINFLIP_ACCOUNTS.with(|cell| {
			let mut map = cell.borrow_mut();
			match map.get_mut(account_id) {
				None => {
					map.insert(
						*account_id,
						ChainflipAccountData { state: ChainflipAccountState::CurrentAuthority },
					);
				},
				Some(item) => {
					(*item).state = ChainflipAccountState::CurrentAuthority;
				},
			}
		});
	}

	fn set_historical_validator(_account_id: &Self::AccountId) {
		todo!("Implement when required");
	}

	fn from_historical_to_backup_or_passive(_account_id: &Self::AccountId) {
		todo!("Implement when required");
	}
}
