use std::cell::RefCell;

use sp_runtime::DispatchError;

use crate::SystemStateInfo;

thread_local! {
	pub static MAINTENANCE: RefCell<bool>  = RefCell::new(false);
}

pub struct MockSystemStateInfo;

impl SystemStateInfo for MockSystemStateInfo {
	fn ensure_no_maintenance() -> Result<(), DispatchError> {
		if MAINTENANCE.with(|cell| *cell.borrow()) {
			Err(DispatchError::Other("We are in maintenance!"))
		} else {
			Ok(())
		}
	}
}

impl MockSystemStateInfo {
	pub fn set_maintenance(mode: bool) {
		MAINTENANCE.with(|cell| *cell.borrow_mut() = mode);
	}
}
