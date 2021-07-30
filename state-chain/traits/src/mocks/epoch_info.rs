use crate::EpochInfo;
use std::cell::RefCell;

type AccountId = u64;
pub struct Mock;

thread_local! {
	pub static CURRENT_VALIDATORS: RefCell<Vec<AccountId>> = RefCell::new(vec![]);
	pub static NEXT_VALIDATORS: RefCell<Vec<AccountId>> = RefCell::new(vec![]);
	pub static BOND: RefCell<u128> = RefCell::new(0);
	pub static EPOCH: RefCell<u32> = RefCell::new(0);
	pub static IS_AUCTION: RefCell<bool> = RefCell::new(false);
}

impl Mock {
	/// Get the current number of validators.
	pub fn validator_count() -> usize {
		CURRENT_VALIDATORS.with(|cell| cell.borrow().len())
	}

	/// Add a validator to the current validators.
	pub fn add_validator(account: AccountId) {
		CURRENT_VALIDATORS.with(|cell| cell.borrow_mut().push(account))
	}

	/// Queue a validator. Adds the validator to the set of next validators.
	pub fn queue_validator(account: AccountId) {
		NEXT_VALIDATORS.with(|cell| cell.borrow_mut().push(account))
	}

	/// Clears the current and next validators.
	pub fn clear_validators() {
		CURRENT_VALIDATORS.with(|cell| cell.borrow_mut().clear());
		NEXT_VALIDATORS.with(|cell| cell.borrow_mut().clear());
	}

	/// Set the bond amount.
	pub fn set_bond(bond: u128) {
		BOND.with(|cell| *(cell.borrow_mut()) = bond);
	}

	/// Set the epoch.
	pub fn set_epoch(epoch: u32) {
		EPOCH.with(|cell| *(cell.borrow_mut()) = epoch);
	}

	/// Increment the epoch.
	pub fn incr_epoch() {
		EPOCH.with(|cell| *(cell.borrow_mut()) += 1);
	}

	pub fn set_is_auction_phase(is_auction: bool) {
		IS_AUCTION.with(|cell| *(cell.borrow_mut()) = is_auction);
	}
}

impl EpochInfo for Mock {
	type ValidatorId = AccountId;
	type Amount = u128;
	type EpochIndex = u32;

	fn current_validators() -> Vec<Self::ValidatorId> {
		CURRENT_VALIDATORS.with(|cell| cell.borrow().clone())
	}

	fn is_validator(account: &Self::ValidatorId) -> bool {
		Self::current_validators().as_slice().contains(account)
	}

	fn bond() -> Self::Amount {
		BOND.with(|cell| *cell.borrow())
	}

	fn next_validators() -> Vec<Self::ValidatorId> {
		NEXT_VALIDATORS.with(|cell| cell.borrow().clone())
	}

	fn epoch_index() -> Self::EpochIndex {
		EPOCH.with(|cell| *cell.borrow())
	}

	fn is_auction_phase() -> bool {
		IS_AUCTION.with(|cell| *cell.borrow())
	}
}
