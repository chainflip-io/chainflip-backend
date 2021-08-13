use crate::{RotationError, VaultRotation};
use std::cell::RefCell;
use std::marker::PhantomData;

type ValidatorId = u64;
thread_local! {
	pub static TO_CONFIRM: RefCell<Result<(), RotationError<()>>> = RefCell::new(Err(RotationError::NotConfirmed));
}

pub struct Mock {}

// Helper function to clear the confirmation result
pub fn clear_confirmation() {
	TO_CONFIRM.with(|l| *l.borrow_mut() = Ok(()));
}

impl VaultRotation for Mock {
	type ValidatorId = ();
	type Amount = ();

	fn start_vault_rotation(
		_winners: Vec<Self::ValidatorId>,
		_min_bid: Self::Amount,
	) -> Result<(), RotationError<Self::ValidatorId>> {
		TO_CONFIRM.with(|l| *l.borrow_mut() = Err(RotationError::NotConfirmed));
		Ok(())
	}

	fn finalize_rotation() -> Result<(), RotationError<Self::ValidatorId>> {
		TO_CONFIRM.with(|l| (*l.borrow()).clone())
	}
}
