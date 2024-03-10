use crate::{EthereumInstance, PolkadotInstance, Runtime};
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};

pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> Weight {
		use cf_runtime_upgrade_utilities::genesis_hashes;
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => {
				log::info!("🧹 Applying housekeeping chores for Berghain.");
				// Remove old duplicated aborted broadcasts storage
				pallet_cf_broadcast::AbortedBroadcasts::<Runtime, EthereumInstance>::mutate(
					|all| all.retain(|id| ![964, 1093].contains(id)),
				);
				pallet_cf_broadcast::AbortedBroadcasts::<Runtime, PolkadotInstance>::mutate(
					|all| all.retain(|id| ![168, 169, 170].contains(id)),
				);
				// Corrupted storage from a previous runtime upgrade.
				frame_support::storage::unhashed::kill(
					// Raw storage key retrieved from try-runtime error message.
					&hex_literal::hex!("09f888937e67e4859e4ee6a943cc9e08347279749a2449c0b500c3a2462071c57f7df985518de74827010000")[..],
				);
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
}
