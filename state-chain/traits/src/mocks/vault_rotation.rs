use crate::VaultRotator;
use std::cell::RefCell;

thread_local! {
	pub static TO_CONFIRM: RefCell<Result<(), MockError>> = RefCell::new(Err(MockError));
	pub static ERROR_ON_START: RefCell<bool> = RefCell::new(false);
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Mock;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct MockError;

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
	type RotationError = MockError;

	fn start_vault_rotation(
		_candidates: Vec<Self::ValidatorId>,
	) -> Result<(), Self::RotationError> {
		if Self::error_on_start() {
			Self::reset_error_on_start();
			return Err(MockError);
		}

		TO_CONFIRM.with(|l| *l.borrow_mut() = Err(MockError));
		Ok(())
	}

	fn finalize_rotation() -> Result<(), Self::RotationError> {
		TO_CONFIRM.with(|l| (*l.borrow()).clone())
	}
}
