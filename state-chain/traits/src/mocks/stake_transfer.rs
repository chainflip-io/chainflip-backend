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

		pub struct MockStakerProvider;
		impl cf_traits::StakerProvider for MockStakerProvider {
			type ValidatorId = $account_id;
			type Amount = $balance;

			fn get_stakers() -> Vec<Bid<Self::ValidatorId, Self::Amount>> {
				BALANCES.with(|cell| {
					cell.borrow().iter().map(|(account_id, balance)| {
						(*account_id, *balance)
					}).collect()
				})
			}
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

			fn stake_updated(validator_id: &Self::ValidatorId, amount: Self::Amount) {
				STAKE_UPDATES.with(|cell| {
					cell.borrow_mut().insert(validator_id.clone(), amount)
				});
			}
		}

		pub struct MockStakeTransfer;

		impl MockStakeTransfer {
			pub fn get_balance(account_id: $account_id) -> $balance {
				BALANCES.with(|cell| {
					cell.borrow()
						.get(&account_id)
						.map(ToOwned::to_owned)
						.unwrap_or_default()
				})
			}
		}

		impl cf_traits::StakeTransfer for MockStakeTransfer {
			type AccountId = $account_id;
			type Balance = $balance;
			type Handler = MockStakeHandler;

			fn stakeable_balance(account_id: &Self::AccountId) -> Self::Balance {
				Self::get_balance(account_id.clone())
			}
			fn claimable_balance(account_id: &Self::AccountId) -> Self::Balance {
				Self::get_balance(account_id.clone())
			}
			fn credit_stake(account_id: &Self::AccountId, amount: Self::Balance) -> Self::Balance {
				BALANCES.with(|cell| *cell.borrow_mut().entry(account_id.clone()).or_default() += amount);
				Self::get_balance(account_id.clone())
			}
			fn try_claim(
				account_id: &Self::AccountId,
				amount: Self::Balance,
			) -> Result<(), sp_runtime::DispatchError> {
				BALANCES.with(|cell| {
					cell.borrow_mut()
						.entry(account_id.clone())
						.or_default()
						.checked_sub(amount)
						.map(|_| ())
						.ok_or("Overflow".into())
				})
			}
			fn settle_claim(_amount: Self::Balance) {
				unimplemented!()
			}
			fn revert_claim(account_id: &Self::AccountId, amount: Self::Balance) {
				Self::credit_stake(account_id, amount);
			}
		}
	};
}
