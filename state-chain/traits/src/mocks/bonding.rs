use crate::Bonding;
use std::{cell::RefCell, collections::HashMap};

use crate::DispatchResult;

pub type Amount = u128;
pub type ValidatorId = u64;

thread_local! {
	pub static AUTHORITY_BONDS: RefCell<HashMap<ValidatorId, Amount>> = RefCell::new(HashMap::default());
}

pub struct MockBonder;

impl MockBonder {
	pub fn get_bond(account_id: &ValidatorId) -> Amount {
		AUTHORITY_BONDS.with(|cell| cell.borrow().get(account_id).copied().unwrap_or(0))
	}
}

impl Bonding for MockBonder {
	type ValidatorId = ValidatorId;
	type Amount = Amount;

	fn update_bond(account_id: &Self::ValidatorId, bond: Self::Amount) {
		AUTHORITY_BONDS.with(|cell| {
			cell.borrow_mut().insert(*account_id, bond);
		})
	}

	fn try_bond(account_id: &Self::ValidatorId, bond: Self::Amount) -> DispatchResult {
		AUTHORITY_BONDS.with(|cell| {
			cell.borrow_mut().insert(*account_id, bond);
		});
		Ok(())
	}

	fn unbond(account_id: &Self::ValidatorId) {
		AUTHORITY_BONDS.with(|cell| {
			cell.borrow_mut().remove(account_id);
		})
	}
}
