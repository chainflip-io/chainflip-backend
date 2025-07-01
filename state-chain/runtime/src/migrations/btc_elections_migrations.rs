use crate::*;
use frame_support::{pallet_prelude::Weight, traits::OnRuntimeUpgrade};

use crate::chainflip::bitcoin_elections;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		Ok(().encode())
	}

	fn on_runtime_upgrade() -> Weight {
		let _result =
			pallet_cf_elections::Pallet::<Runtime, BitcoinInstance>::internally_initialize(
				bitcoin_elections::initial_state(),
			);
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		use pallet_cf_elections::{
			electoral_systems::{
				block_height_witnesser::BlockHeightWitnesserSettings,
				block_witnesser::state_machine::BlockWitnesserSettings,
			},
			ElectoralUnsynchronisedSettings, SharedDataReferenceLifetime,
		};

		let unsynchronized_settings =
			ElectoralUnsynchronisedSettings::<Runtime, BitcoinInstance>::get();
		assert_eq!(
			unsynchronized_settings,
			Some((
				BlockHeightWitnesserSettings { safety_buffer: 8 },
				BlockWitnesserSettings {
					max_ongoing_elections: 15,
					max_optimistic_elections: 1,
					safety_margin: 1,
					safety_buffer: 8,
				},
				BlockWitnesserSettings {
					max_ongoing_elections: 15,
					max_optimistic_elections: 1,
					safety_margin: 1,
					safety_buffer: 8,
				},
				BlockWitnesserSettings {
					max_ongoing_elections: 15,
					max_optimistic_elections: 1,
					safety_margin: 0,
					safety_buffer: 8,
				},
				Default::default(),
				(),
			))
		);

		let lifetime = SharedDataReferenceLifetime::<Runtime, BitcoinInstance>::get();
		assert_eq!(lifetime, 8);
		Ok(())
	}
}
