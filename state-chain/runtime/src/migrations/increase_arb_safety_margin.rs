use crate::Runtime;
use cf_chains::instances::ArbitrumInstance;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use pallet_cf_ingress_egress::WitnessSafetyMargin;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

use codec::{Decode, Encode};

pub struct Migration;

// Keeping it a multiple of 24 to match the witness period of Arbitrum
const NEW_ARB_SAFETY_MARGIN: u64 = 672;

impl OnRuntimeUpgrade for Migration {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		log::info!("🚀 Increasing Arbitrum safety margin");
		let arb_margin = WitnessSafetyMargin::<Runtime, ArbitrumInstance>::get();
		Ok(arb_margin.encode())
	}

	fn on_runtime_upgrade() -> Weight {
		WitnessSafetyMargin::<Runtime, ArbitrumInstance>::put(NEW_ARB_SAFETY_MARGIN);
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let old_arb_margin: Option<u64> = Decode::decode(&mut &state[..])
			.map_err(|_| DispatchError::Other("Failed to decode state"))?;
		let new_arb_margin = WitnessSafetyMargin::<Runtime, ArbitrumInstance>::get();
		assert_eq!(new_arb_margin, Some(NEW_ARB_SAFETY_MARGIN));
		assert_ne!(old_arb_margin, new_arb_margin);
		log::info!("✅ Successfully increased Arbitrum safety margin");
		Ok(())
	}
}
