use std::{borrow::Borrow, cell::RefCell};
use super::AccountId;

pub struct Mock;

thread_local! {
	pub static CURRENT_VALIDATORS: RefCell<Vec<AccountId>> = RefCell::new(vec![]);
}

impl Mock {
	pub fn validator_count() -> usize {
		CURRENT_VALIDATORS.with(|cell| cell.borrow().len())
	}

	pub fn add_validator(account: AccountId) {
		CURRENT_VALIDATORS.with(|cell| cell.borrow_mut().push(account))
	}

	pub fn reset() {
		CURRENT_VALIDATORS.with(|cell| cell.borrow_mut().clear())
	}
}

impl cf_traits::ValidatorProvider for Mock {
    type ValidatorId = AccountId;

    fn current_validators() -> Vec<Self::ValidatorId> {
        CURRENT_VALIDATORS.with(|cell| cell.borrow().clone())
    }

    fn is_validator(account: &Self::ValidatorId) -> bool {
        Self::current_validators().as_slice().contains(account)
    }
}