use crate::{ChainflipAccount, ChainflipAccountData, ChainflipAccountState, EpochIndex};
use std::cell::RefCell;
use std::collections::HashMap;
thread_local! {
	pub static CHAINFLIP_ACCOUNTS: RefCell<HashMap<u64, ChainflipAccountData>> = RefCell::new(HashMap::new());
}

pub struct MockChainflipAccount;

impl ChainflipAccount for MockChainflipAccount {
	type AccountId = u64;

	fn get(account_id: &Self::AccountId) -> ChainflipAccountData {
		CHAINFLIP_ACCOUNTS.with(|cell| *cell.borrow().get(account_id).unwrap())
	}

	fn update_state(account_id: &Self::AccountId, state: ChainflipAccountState) {
		CHAINFLIP_ACCOUNTS.with(|cell| {
			let mut map = cell.borrow_mut();
			match map.get_mut(account_id) {
				None => {
					map.insert(
						*account_id,
						ChainflipAccountData {
							state,
							last_active_epoch: None,
						},
					);
				}
				Some(item) => (*item).state = state,
			}
		});
	}

	fn update_last_active_epoch(account_id: &Self::AccountId, index: EpochIndex) {
		CHAINFLIP_ACCOUNTS.with(|cell| {
			let mut map = cell.borrow_mut();
			match map.get_mut(account_id) {
				None => {
					map.insert(
						*account_id,
						ChainflipAccountData {
							state: ChainflipAccountState::Passive,
							last_active_epoch: Some(index),
						},
					);
				}
				Some(item) => (*item).last_active_epoch = Some(index),
			}
		});
	}
}