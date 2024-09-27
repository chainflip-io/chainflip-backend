use cf_chains::instances::{
	ArbitrumInstance, BitcoinInstance, EthereumInstance, PolkadotInstance, SolanaInstance,
};
use cf_runtime_upgrade_utilities::VersionedMigration;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use pallet_cf_broadcast::AbortedBroadcasts;
use sp_runtime::DispatchError;

use crate::*;

pub type AllInstancesMigration = (
	// Only Eth and Arb have stale aborted broadcasts
	VersionedMigration<
		pallet_cf_broadcast::Pallet<Runtime, EthereumInstance>,
		EthereumMigration,
		9,
		10,
	>,
	VersionedMigration<
		pallet_cf_broadcast::Pallet<Runtime, ArbitrumInstance>,
		ArbitrumMigration,
		9,
		10,
	>,
	VersionedMigration<pallet_cf_broadcast::Pallet<Runtime, SolanaInstance>, NoopUpgrade, 9, 10>,
	VersionedMigration<pallet_cf_broadcast::Pallet<Runtime, PolkadotInstance>, NoopUpgrade, 9, 10>,
	VersionedMigration<pallet_cf_broadcast::Pallet<Runtime, BitcoinInstance>, NoopUpgrade, 9, 10>,
);

// Stale aborted broadcasts on mainnet as of 23/09/2024
const ETHEREUM_ABORTED_BROADCASTS: [BroadcastId; 5] = [3026, 3684, 3686, 11350, 11592];
const ARBITRUM_ABORTED_BROADCASTS: [BroadcastId; 5] = [238, 239, 345, 423, 426];

pub struct EthereumMigration;
pub struct ArbitrumMigration;

impl OnRuntimeUpgrade for EthereumMigration {
	fn on_runtime_upgrade() -> Weight {
		AbortedBroadcasts::<Runtime, EthereumInstance>::mutate(|aborted| {
			ETHEREUM_ABORTED_BROADCASTS.iter().for_each(|broadcast_id| {
				if aborted.remove(broadcast_id) {
					log::info!("Removed Ethereum aborted broadcast {}", broadcast_id);
				}
			});
		});
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		let aborted_broadcasts = AbortedBroadcasts::<Runtime, EthereumInstance>::get();
		ETHEREUM_ABORTED_BROADCASTS.iter().for_each(|broadcast_id| {
			assert!(
				!aborted_broadcasts.contains(broadcast_id),
				"Aborted broadcast {broadcast_id} still exists"
			);
		});
		Ok(())
	}
}

impl OnRuntimeUpgrade for ArbitrumMigration {
	fn on_runtime_upgrade() -> Weight {
		AbortedBroadcasts::<Runtime, ArbitrumInstance>::mutate(|aborted| {
			ARBITRUM_ABORTED_BROADCASTS.iter().for_each(|broadcast_id| {
				if aborted.remove(broadcast_id) {
					log::info!("Removed Arbitrum aborted broadcast {}", broadcast_id);
				}
			});
		});
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		let aborted_broadcasts = AbortedBroadcasts::<Runtime, ArbitrumInstance>::get();
		ARBITRUM_ABORTED_BROADCASTS.iter().for_each(|broadcast_id| {
			assert!(
				!aborted_broadcasts.contains(broadcast_id),
				"Aborted broadcast {broadcast_id} still exists"
			);
		});
		Ok(())
	}
}
