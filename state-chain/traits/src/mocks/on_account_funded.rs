#[macro_export]
macro_rules! impl_mock_on_account_funded {
	($account_id:ty, $balance:ty) => {
		type OnAccountFundedBalances = std::collections::HashMap<$account_id, $balance>;
		type OnAccountFundedUpdates = std::collections::HashMap<$account_id, $balance>;

		thread_local! {
			pub static BALANCES: std::cell::RefCell<OnAccountFundedBalances> = std::cell::RefCell::new(OnAccountFundedBalances::default());
			pub static PENDING_REDEMPTIONS: std::cell::RefCell<OnAccountFundedBalances> = std::cell::RefCell::new(OnAccountFundedBalances::default());
			pub static FUNDING_UPDATES: std::cell::RefCell<OnAccountFundedUpdates> = std::cell::RefCell::new(OnAccountFundedUpdates::default());
		}

		pub struct MockOnAccountFunded;
		impl MockOnAccountFunded {
			// Check if updated and reset
			pub fn has_account_been_funded(account_id: &$account_id) -> bool {
				FUNDING_UPDATES.with(|cell| {
					cell.borrow_mut().remove(&account_id).is_some()
				})
			}
		}

		impl cf_traits::OnAccountFunded for MockOnAccountFunded {
			type ValidatorId = $account_id;
			type Amount = $balance;

			fn on_account_funded(validator_id: &Self::ValidatorId, amount: Self::Amount) {
				FUNDING_UPDATES.with(|cell| {
					cell.borrow_mut().insert(validator_id.clone(), amount)
				});
			}
		}
	};
}
