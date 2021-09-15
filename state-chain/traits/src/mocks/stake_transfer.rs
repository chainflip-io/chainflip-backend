#[macro_export]
macro_rules! impl_mock_stake_transfer {
	($account_id:ty, $balance:ty) => {
		type StakeTransferBalances = std::collections::HashMap<$account_id, $balance>;
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

		thread_local! {
			pub static BALANCES: std::cell::RefCell<StakeTransferBalances> = std::cell::RefCell::new(StakeTransferBalances::default());
			pub static PENDING_CLAIMS: std::cell::RefCell<StakeTransferBalances> = std::cell::RefCell::new(StakeTransferBalances::default());
		}

		impl cf_traits::StakeTransfer for MockStakeTransfer {
			type AccountId = $account_id;
			type Balance = $balance;

			fn stakeable_balance(account_id: &Self::AccountId) -> Self::Balance {
				Self::get_balance(*account_id)
			}
			fn claimable_balance(account_id: &Self::AccountId) -> Self::Balance {
				Self::get_balance(*account_id)
			}
			fn credit_stake(account_id: &Self::AccountId, amount: Self::Balance) -> Self::Balance {
				BALANCES.with(|cell| *cell.borrow_mut().entry(*account_id).or_default() += amount);
				Self::get_balance(*account_id)
			}
			fn try_claim(
				account_id: &Self::AccountId,
				amount: Self::Balance,
			) -> Result<(), sp_runtime::DispatchError> {
				BALANCES.with(|cell| {
					cell.borrow_mut()
						.entry(*account_id)
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
			fn update_validator_bonds(new_validators: &Vec<Self::AccountId>, new_bond: Self::Balance) {
				unimplemented!()
			}
		}
	};
}
