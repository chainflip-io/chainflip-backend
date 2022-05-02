use crate::Runtime;
use frame_support::{traits::OnRuntimeUpgrade, weights::RuntimeDbWeight};

const NULL_ADDRESS: [u8; 20] = [0u8; 20];

// TODO: Set this to a non-null value when we deploy the contracts.
const STAKE_MANAGER_ADDRESS: [u8; 20] = NULL_ADDRESS;
const KEY_MANAGER_ADDRESS: [u8; 20] = NULL_ADDRESS;

/// A migration that updates the addresses for contracts.
pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		pallet_cf_environment::StakeManagerAddress::<Runtime>::put(STAKE_MANAGER_ADDRESS);
		pallet_cf_environment::KeyManagerAddress::<Runtime>::put(KEY_MANAGER_ADDRESS);

		RuntimeDbWeight::default().reads_writes(0, 3)
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		if pallet_cf_environment::StakeManagerAddress::<Runtime>::get() == NULL_ADDRESS {
			return Err("StakeManagerAddress not set.")
		}
		if pallet_cf_environment::KeyManagerAddress::<Runtime>::get() == NULL_ADDRESS {
			return Err("KeyManagerAddress not set.")
		}
		Ok(())
	}
}
