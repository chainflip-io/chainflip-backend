use crate::{ChainflipAccount, ChainflipAccountData, ChainflipAccountState};
use std::{cell::RefCell, collections::HashMap};
thread_local! {
	pub static CHAINFLIP_ACCOUNTS: RefCell<HashMap<u64, ChainflipAccountData>> = RefCell::new(HashMap::new());
}

/// An implementation of ChainflipAccount that stores data in a thread-local variable. Useful
/// for running tests outside of an externalities environment.
///
/// If running inside an externalities environment, use [cf_traits::ChainflipAccountStore] instead.
pub struct MockChainflipAccount;

impl MockChainflipAccount {
	pub fn is_backup(account_id: &u64) -> bool {
		matches!(
			Self::get(account_id).state,
			ChainflipAccountState::HistoricalAuthority | ChainflipAccountState::Backup
		)
	}
}

impl ChainflipAccount for MockChainflipAccount {
	type AccountId = u64;

	fn get(account_id: &Self::AccountId) -> ChainflipAccountData {
		CHAINFLIP_ACCOUNTS.with(|cell| cell.borrow().get(account_id).cloned().unwrap_or_default())
	}

	fn set_current_authority(account_id: &Self::AccountId) {
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

	fn set_historical_authority(_account_id: &Self::AccountId) {
		todo!("Implement when required");
	}

	fn from_historical_to_backup(_account_id: &Self::AccountId) {
		todo!("Implement when required");
	}
}
