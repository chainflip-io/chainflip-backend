use crate::Runtime;
use cf_chains::instances::{ArbitrumInstance, EthereumInstance, PolkadotInstance};
use cf_runtime_utilities::genesis_hashes;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use pallet_cf_broadcast::migrations::remove_aborted_broadcasts;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> Weight {
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => {
				log::info!("🧹 Housekeeping, removing stale aborted broadcasts");
				remove_aborted_broadcasts::remove_stale_and_all_older::<Runtime, EthereumInstance>(
					remove_aborted_broadcasts::ETHEREUM_MAX_ABORTED_BROADCAST_BERGHAIN,
				);
				remove_aborted_broadcasts::remove_stale_and_all_older::<Runtime, ArbitrumInstance>(
					remove_aborted_broadcasts::ARBITRUM_MAX_ABORTED_BROADCAST_BERGHAIN,
				);
			},
			genesis_hashes::PERSEVERANCE => {
				log::info!("🧹 Housekeeping, removing stale aborted broadcasts");
				remove_aborted_broadcasts::remove_stale_and_all_older::<Runtime, EthereumInstance>(
					remove_aborted_broadcasts::ETHEREUM_MAX_ABORTED_BROADCAST_PERSEVERANCE,
				);
				remove_aborted_broadcasts::remove_stale_and_all_older::<Runtime, ArbitrumInstance>(
					remove_aborted_broadcasts::ARBITRUM_MAX_ABORTED_BROADCAST_PERSEVERANCE,
				);
				remove_aborted_broadcasts::remove_stale_and_all_older::<Runtime, PolkadotInstance>(
					remove_aborted_broadcasts::POLKADOT_MAX_ABORTED_BROADCAST_PERSEVERANCE,
				);
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
				log::info!(
					"Housekeeping post_upgrade, checking stale aborted broadcasts are removed."
				);
				remove_aborted_broadcasts::assert_removed::<Runtime, EthereumInstance>(
					remove_aborted_broadcasts::ETHEREUM_MAX_ABORTED_BROADCAST_BERGHAIN,
				);
				remove_aborted_broadcasts::assert_removed::<Runtime, ArbitrumInstance>(
					remove_aborted_broadcasts::ARBITRUM_MAX_ABORTED_BROADCAST_BERGHAIN,
				);
			},
			genesis_hashes::PERSEVERANCE => {
				log::info!(
					"Housekeeping post_upgrade, checking stale aborted broadcasts are removed."
				);
				remove_aborted_broadcasts::assert_removed::<Runtime, EthereumInstance>(
					remove_aborted_broadcasts::ETHEREUM_MAX_ABORTED_BROADCAST_PERSEVERANCE,
				);
				remove_aborted_broadcasts::assert_removed::<Runtime, ArbitrumInstance>(
					remove_aborted_broadcasts::ARBITRUM_MAX_ABORTED_BROADCAST_PERSEVERANCE,
				);
				remove_aborted_broadcasts::assert_removed::<Runtime, PolkadotInstance>(
					remove_aborted_broadcasts::POLKADOT_MAX_ABORTED_BROADCAST_PERSEVERANCE,
				);
			},
			genesis_hashes::SISYPHOS => {
				log::info!("Skipping housekeeping post_upgrade for Sisyphos.");
			},
			_ => {},
		}
		Ok(())
	}
}
