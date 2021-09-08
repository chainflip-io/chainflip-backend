#[macro_export]
macro_rules! impl_mock_staker_handler {
	($account_id:ty, $balance:ty) => {
		type StakeBalances = std::collections::HashMap<$account_id, $balance>;
		pub struct MockStakerHandler;

		impl MockStakerHandler {
			// Check is updated and reset
			pub fn has_stake_updated(account_id: $account_id) -> bool {
				STAKES.with(|cell| {
					cell.borrow_mut().remove(&account_id).is_some()
				})
			}
		}

		impl StakerHandler for MockStakerHandler {
			type ValidatorId = $account_id;
			type Amount = $balance;

			fn stake_updated(validator_id: Self::ValidatorId, amount: Self::Amount) {
				STAKES.with(|cell| {
					cell.borrow_mut()
						.insert(validator_id, amount)
				});
			}
		}

		thread_local! {
			pub static STAKES: std::cell::RefCell<StakeBalances> = std::cell::RefCell::new(StakeBalances::default());
		}
	};
}
