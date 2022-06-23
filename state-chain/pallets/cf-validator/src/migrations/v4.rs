use crate::*;
use cf_runtime_upgrade_utilities::move_storage;
use frame_support::storage::{migration::*, storage_prefix};
use sp_std::marker::PhantomData;

pub struct Migration<T: Config>(PhantomData<T>);

const VALIDATOR_PALLET_NAME: &[u8] = b"Validator";

frame_support::generate_storage_alias!(
	Validator, EmergencyRotationRequested => Value<bool>
);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		move_storage(
			VALIDATOR_PALLET_NAME,
			b"RotationPhase",
			VALIDATOR_PALLET_NAME,
			b"CurrentRotationPhase",
		);

		EmergencyRotationRequested::kill();

		T::DbWeight::get().reads_writes(1, 1)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<(), &'static str> {
		ensure!(
			!CurrentRotationPhase::<T>::exists(),
			"Expected CurrentRotationPhase to be empty before upgrade",
		);
		Ok(())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		ensure!(
			CurrentRotationPhase::<T>::exists(),
			"Expected CurrentRotationPhase to be non-empty after upgrade",
		);
		Ok(())
	}
}
