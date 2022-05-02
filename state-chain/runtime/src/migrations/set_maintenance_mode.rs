use crate::Runtime;
use cf_traits::SystemStateInfo;
use frame_support::{traits::OnRuntimeUpgrade, weights::RuntimeDbWeight};
use pallet_cf_environment::{CurrentSystemState, SystemState};

/// A migration that puts the system into maintenance mode.
pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		CurrentSystemState::<Runtime>::put(SystemState::Maintenance);
		RuntimeDbWeight::default().writes(1)
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		use frame_support::ensure;
		use pallet_cf_environment::SystemStateProvider;

		ensure!(
			SystemStateProvider::<Runtime>::ensure_no_maintenance().is_err(),
			"Failed to set maintenance mode"
		);
		Ok(())
	}
}
