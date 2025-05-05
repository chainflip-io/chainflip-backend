use crate::*;
use frame_support::{pallet_prelude::Weight, traits::OnRuntimeUpgrade};

use pallet_cf_elections::{ElectoralUnsynchronisedSettings, SharedDataReferenceLifetime};

use pallet_cf_elections::electoral_systems::block_witnesser::state_machine::BlockWitnesserSettings;

#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
use crate::chainflip::bitcoin_elections;

pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		Ok(().encode())
	}

	fn on_runtime_upgrade() -> Weight {
		let _result = pallet_cf_elections::Pallet::<Runtime, BitcoinInstance>::internally_initialize(
			bitcoin_elections::initial_state()
		);
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		let unsynchronized_settings =
			ElectoralUnsynchronisedSettings::<Runtime, BitcoinInstance>::get();
		assert_eq!(
			unsynchronized_settings,
			Some((
				Default::default(),
				BlockWitnesserSettings { max_concurrent_elections: 15, safety_margin: 3 },
				BlockWitnesserSettings { max_concurrent_elections: 15, safety_margin: 3 },
				BlockWitnesserSettings { max_concurrent_elections: 15, safety_margin: 0 },
				Default::default(),
				(),
			))
		);

		let lifetime = SharedDataReferenceLifetime::<Runtime, BitcoinInstance>::get();
		assert_eq!(lifetime, 8);
		Ok(())
	}
}
