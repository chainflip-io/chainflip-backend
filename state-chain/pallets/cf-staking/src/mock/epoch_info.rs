use std::cell::RefCell;
use super::AccountId;

pub struct Mock;

thread_local! {
	pub static CURRENT_VALIDATORS: RefCell<Vec<AccountId>> = RefCell::new(vec![]);
	pub static BOND: RefCell<u128> = RefCell::new(0);
}

impl Mock {
	pub fn validator_count() -> usize {
		CURRENT_VALIDATORS.with(|cell| cell.borrow().len())
	}

	pub fn add_validator(account: AccountId) {
		CURRENT_VALIDATORS.with(|cell| cell.borrow_mut().push(account))
	}

	pub fn clear_validators() {
		CURRENT_VALIDATORS.with(|cell| cell.borrow_mut().clear())
	}

	pub fn set_bond(bond: u128) {
		BOND.with(|cell| *(cell.borrow_mut()) = bond);
	}
}

impl cf_traits::EpochInfo for Mock {
	type ValidatorId = AccountId;
	type Amount = u128;
	type EpochIndex = u64;

	fn current_validators() -> Vec<Self::ValidatorId> {
		CURRENT_VALIDATORS.with(|cell| cell.borrow().clone())
	}

	fn is_validator(account: &Self::ValidatorId) -> bool {
		Self::current_validators().as_slice().contains(account)
	}
	
	fn bond() -> Self::Amount {
		BOND.with(|cell| cell.borrow().clone())
	}

	// The following two are not used by the staking pallet
	fn next_validators() -> Vec<Self::ValidatorId> {
		unimplemented!()
	}

	fn epoch_index() -> Self::EpochIndex {
		unimplemented!()
	}
}