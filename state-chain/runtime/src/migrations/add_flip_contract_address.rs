use crate::Runtime;
use frame_support::{traits::OnRuntimeUpgrade, weights::RuntimeDbWeight};

const NULL_ADDRESS: [u8; 20] = [0u8; 20];

// TODO: Set this to a non-null value when we deploy the contract.
const FLIP_TOKEN_ADDRESS: [u8; 20] = NULL_ADDRESS;

/// A migration that sets the Flip token address.
pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		pallet_cf_environment::FlipTokenAddress::<Runtime>::put(FLIP_TOKEN_ADDRESS);

		RuntimeDbWeight::default().reads_writes(0, 1)
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		if pallet_cf_environment::FlipTokenAddress::<Runtime>::get() == NULL_ADDRESS {
			Err("FlipTokenAddress not set.")
		} else {
			Ok(())
		}
	}
}
