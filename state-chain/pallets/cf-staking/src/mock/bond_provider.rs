use std::cell::RefCell;

thread_local! {
	pub static BOND: RefCell<u128> = RefCell::new(0);
}

#[derive(Default)]
pub struct Mock;

impl Mock {
	pub fn set_bond(bond: u128) {
		BOND.with(|cell| *(cell.borrow_mut()) = bond);
	}
}

impl cf_traits::BondProvider for Mock {
	type Amount = u128;

	fn current_bond() -> Self::Amount {
		BOND.with(|cell| cell.borrow().clone())
	}
}