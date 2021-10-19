use crate::{RotationError, VaultRotator};
use std::cell::RefCell;

thread_local! {
	pub static TO_CONFIRM: RefCell<Result<(), RotationError<u64>>> = RefCell::new(Err(RotationError::NotConfirmed));
	pub static ERROR_ON_START: RefCell<bool> = RefCell::new(false);
}

pub struct Mock {}

// Helper function to clear the confirmation result
pub fn clear_confirmation() {
	TO_CONFIRM.with(|l| *l.borrow_mut() = Ok(()));
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

	fn start_vault_rotation(
		_candidates: Vec<Self::ValidatorId>,
	) -> Result<(), RotationError<Self::ValidatorId>> {
		if Self::error_on_start() {
			Self::reset_error_on_start();
			return Err(RotationError::FailedToMakeKeygenRequest);
		}

		TO_CONFIRM.with(|l| *l.borrow_mut() = Err(RotationError::NotConfirmed));
		Ok(())
	}

	fn finalize_rotation() -> Result<(), RotationError<Self::ValidatorId>> {
		TO_CONFIRM.with(|l| (*l.borrow()).clone())
	}
}
