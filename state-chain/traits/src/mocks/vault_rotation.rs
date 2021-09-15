use crate::{RotationError, VaultRotator};
use std::cell::RefCell;

thread_local! {
	pub static TO_CONFIRM: RefCell<Result<(), RotationError<u64>>> = RefCell::new(Err(RotationError::NotConfirmed));
}

pub struct Mock {}

// Helper function to clear the confirmation result
pub fn clear_confirmation() {
	TO_CONFIRM.with(|l| *l.borrow_mut() = Ok(()));
}

impl VaultRotator for Mock {
	type ValidatorId = u64;

	fn start_vault_rotation(
		_candidates: Vec<Self::ValidatorId>,
	) -> Result<(), RotationError<Self::ValidatorId>> {
		TO_CONFIRM.with(|l| *l.borrow_mut() = Err(RotationError::NotConfirmed));
		Ok(())
	}

	fn finalize_rotation() -> Result<(), RotationError<Self::ValidatorId>> {
		TO_CONFIRM.with(|l| (*l.borrow()).clone())
	}
}
