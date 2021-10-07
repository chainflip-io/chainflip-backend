use crate::VaultRotator;
use std::cell::RefCell;

thread_local! {
	pub static TO_CONFIRM: RefCell<Result<(), MockError>> = RefCell::new(Err(MockError));
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Mock;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct MockError;

// Helper function to clear the confirmation result
pub fn clear_confirmation() {
	TO_CONFIRM.with(|l| *l.borrow_mut() = Ok(()));
}

impl VaultRotator for Mock {
	type ValidatorId = u64;
	type RotationError = MockError;

	fn start_vault_rotation(
		_candidates: Vec<Self::ValidatorId>,
	) -> Result<(), Self::RotationError> {
		TO_CONFIRM.with(|l| *l.borrow_mut() = Err(MockError));
		Ok(())
	}

	fn finalize_rotation() -> Result<(), Self::RotationError> {
		TO_CONFIRM.with(|l| (*l.borrow()).clone())
	}
}
