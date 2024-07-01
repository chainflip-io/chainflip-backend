use crate::{PolkadotInstance, Runtime};
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};

pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> Weight {
		use cf_runtime_upgrade_utilities::genesis_hashes;
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => {
				log::info!("🧹 Applying housekeeping chores for Berghain.");
				if crate::VERSION.spec_version == 145 {
					// Re-sign polkadot broadcasts.
					// Both pending and aborted broadcasts are re-signed because both would have
					// been invalid.
					for broadcast_id in [
						pallet_cf_broadcast::AbortedBroadcasts::<Runtime, PolkadotInstance>::take(),
						pallet_cf_broadcast::PendingBroadcasts::<Runtime, PolkadotInstance>::take()
							.into_iter()
							.collect(),
					]
					.concat()
					{
						if let Some((api_call, ..)) = pallet_cf_broadcast::ThresholdSignatureData::<
							Runtime,
							PolkadotInstance,
						>::get(broadcast_id)
						{
							pallet_cf_broadcast::Pallet::<Runtime, PolkadotInstance>::clean_up_broadcast_storage(broadcast_id);
							pallet_cf_broadcast::Pallet::<Runtime, PolkadotInstance>::threshold_sign(
								api_call,
								broadcast_id,
								true,
							);
						}
					}
				}
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
