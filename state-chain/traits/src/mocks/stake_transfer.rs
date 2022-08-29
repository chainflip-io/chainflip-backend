#[macro_export]
macro_rules! impl_mock_stake_transfer {
	($account_id:ty, $balance:ty) => {
		type StakeTransferBalances = std::collections::HashMap<$account_id, $balance>;
		type StakeUpdates = std::collections::HashMap<$account_id, $balance>;

		thread_local! {
			pub static BALANCES: std::cell::RefCell<StakeTransferBalances> = std::cell::RefCell::new(StakeTransferBalances::default());
			pub static PENDING_CLAIMS: std::cell::RefCell<StakeTransferBalances> = std::cell::RefCell::new(StakeTransferBalances::default());
			pub static STAKE_UPDATES: std::cell::RefCell<StakeUpdates> = std::cell::RefCell::new(StakeUpdates::default());
		}

		pub struct MockStakeHandler;
		impl MockStakeHandler {
			// Check if updated and reset
			pub fn has_stake_updated(account_id: &$account_id) -> bool {
				STAKE_UPDATES.with(|cell| {
					cell.borrow_mut().remove(&account_id).is_some()
				})
			}
		}

		impl cf_traits::StakeHandler for MockStakeHandler {
			type ValidatorId = $account_id;
			type Amount = $balance;

			fn on_stake_updated(validator_id: &Self::ValidatorId, amount: Self::Amount) {
				STAKE_UPDATES.with(|cell| {
					cell.borrow_mut().insert(validator_id.clone(), amount)
				});
			}
		}
	};
}
