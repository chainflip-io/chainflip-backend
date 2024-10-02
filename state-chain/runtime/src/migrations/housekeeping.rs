use crate::{migrations::remove_aborted_broadcasts, Runtime};
use cf_runtime_upgrade_utilities::genesis_hashes;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> Weight {
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => {
				log::info!("🧹 Housekeeping, removing stale aborted broadcasts");
				remove_aborted_broadcasts::EthereumMigration::on_runtime_upgrade();
				remove_aborted_broadcasts::ArbitrumMigration::on_runtime_upgrade();
			},
			genesis_hashes::PERSEVERANCE => {
				log::info!("🧹 No housekeeping required for Perseverance.");
			},
			genesis_hashes::SISYPHOS => {
				log::info!("🧹 No housekeeping required for Sisyphos.");
			},
			_ => {},
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => {
				remove_aborted_broadcasts::EthereumMigration::post_upgrade();
				remove_aborted_broadcasts::ArbitrumMigration::post_upgrade();
			},
			genesis_hashes::PERSEVERANCE => {
				log::info!("Skipping housekeeping post_upgrade for Perseverance.");
			},
			genesis_hashes::SISYPHOS => {
				log::info!("Skipping housekeeping post_upgrade for Sisyphos.");
			},
			_ => {},
		}
		Ok(())
	}
}
