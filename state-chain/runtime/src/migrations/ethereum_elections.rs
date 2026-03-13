use crate::*;
use frame_support::{pallet_prelude::Weight, traits::OnRuntimeUpgrade};

use crate::chainflip::witnessing::ethereum_elections;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		Ok(().encode())
	}

	fn on_runtime_upgrade() -> Weight {
		let result =
			pallet_cf_elections::Pallet::<Runtime, EthereumInstance>::internally_initialize(
				ethereum_elections::initial_state(),
			);
		if result.is_err() {
			log::info!("✅ Ethereum election pallet already initialized.");
		}
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		use pallet_cf_elections::SharedDataReferenceLifetime;

		let lifetime = SharedDataReferenceLifetime::<Runtime, EthereumInstance>::get();
		assert_eq!(lifetime, 8);

		Ok(())
	}
}
