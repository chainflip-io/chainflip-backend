use crate::*;
use frame_support::{pallet_prelude::Weight, storage::unhashed, traits::OnRuntimeUpgrade};
use frame_system::pallet_prelude::BlockNumberFor;

use pallet_cf_elections::{electoral_system::ElectoralSystem, Config, ElectoralSettings};
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

use codec::{Decode, Encode};

pub struct LivenessSettingsMigration;

const LIVENESS_CHECK_DURATION: BlockNumberFor<Runtime> = 10;

// Because the Liveness electoral system is added to the end, and the rest of its types are the same
// we can simply append the encoded bytes to the raw storage.
impl OnRuntimeUpgrade for LivenessSettingsMigration {
	fn on_runtime_upgrade() -> Weight {
		for key in ElectoralSettings::<Runtime, SolanaInstance>::iter_keys() {
			let mut raw_storage_at_key = unhashed::get_raw(&ElectoralSettings::<
				Runtime,
				SolanaInstance,
			>::hashed_key_for(key))
			.expect("We just got the keys directly from the storage");
			raw_storage_at_key.extend(LIVENESS_CHECK_DURATION.encode());
			ElectoralSettings::<Runtime, SolanaInstance>::insert(key, <<Runtime as Config<SolanaInstance>>::ElectoralSystem as ElectoralSystem>::ElectoralSettings::decode(&mut &raw_storage_at_key[..]).unwrap());
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		for (.., liveness_duration) in ElectoralSettings::<Runtime, SolanaInstance>::iter_values() {
			assert_eq!(liveness_duration, LIVENESS_CHECK_DURATION);
		}
		Ok(())
	}
}
