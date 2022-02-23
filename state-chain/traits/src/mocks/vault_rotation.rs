use sp_runtime::DispatchError;

use crate::{KeygenStatus, VaultRotator};
use std::cell::RefCell;

thread_local! {
	pub static KEYGEN_STATUS: RefCell<Option<KeygenStatus>> = RefCell::new(None);
	pub static ERROR_ON_START: RefCell<bool> = RefCell::new(false);
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Mock;

// Helper function to clear the confirmation result
pub fn clear_confirmation() {
	KEYGEN_STATUS.with(|l| *l.borrow_mut() = None);
}

impl Mock {
	pub fn error_on_start_vault_rotation() {
		ERROR_ON_START.with(|cell| *cell.borrow_mut() = true);
	}
	fn reset_error_on_start() {
		ERROR_ON_START.with(|cell| *cell.borrow_mut() = false);
	}
	fn error_on_start() -> bool {
		ERROR_ON_START.with(|cell| *cell.borrow())
	}
}

impl VaultRotator for Mock {
	type ValidatorId = u64;
	type RotationError = DispatchError;

	fn start_vault_rotation(
		_candidates: Vec<Self::ValidatorId>,
	) -> Result<(), Self::RotationError> {
		if Self::error_on_start() {
			Self::reset_error_on_start();
			return DispatchError::Other("failure").into()
		}

		KEYGEN_STATUS.with(|l| *l.borrow_mut() = Some(KeygenStatus::Busy));
		Ok(())
	}

	fn get_keygen_status() -> Option<KeygenStatus> {
		KEYGEN_STATUS.with(|l| (*l.borrow()).clone())
	}
}
