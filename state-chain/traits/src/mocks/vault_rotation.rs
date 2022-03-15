use sp_runtime::DispatchError;

use crate::{KeygenStatus, VaultRotator};
use std::cell::RefCell;

thread_local! {
	pub static KEYGEN_STATUS: RefCell<Option<KeygenStatus>> = RefCell::new(None);
	pub static ERROR_ON_START: RefCell<bool> = RefCell::new(false);
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Mock;

impl Mock {
	pub fn set_status(s: Option<KeygenStatus>) {
		KEYGEN_STATUS.with(|cell| *cell.borrow_mut() = s);
	}

	fn get_status() -> Option<KeygenStatus> {
		KEYGEN_STATUS.with(|l| (*l.borrow()).clone())
	}

	pub fn set_failed() {
		Self::set_status(Some(KeygenStatus::Failed))
	}

	pub fn set_busy() {
		Self::set_status(Some(KeygenStatus::Busy))
	}

	pub fn set_rotation_complete() {
		assert_eq!(Self::get_status(), Some(KeygenStatus::Busy));
		Self::set_status(None);
	}

	pub fn set_error_on_start(e: bool) {
		ERROR_ON_START.with(|cell| *cell.borrow_mut() = e);
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
			return DispatchError::Other("failure").into()
		}

		Self::set_status(Some(KeygenStatus::Busy));
		Ok(())
	}

	fn get_keygen_status() -> Option<KeygenStatus> {
		Self::get_status()
	}

	fn finalise_rotation() {
		Self::set_status(None);
	}
}
