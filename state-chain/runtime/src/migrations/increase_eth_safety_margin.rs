use crate::Runtime;
use cf_chains::instances::EthereumInstance;
use cf_runtime_upgrade_utilities::genesis_hashes;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use pallet_cf_ingress_egress::WitnessSafetyMargin;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

use codec::{Decode, Encode};

pub struct Migration;

const NEW_ETH_SAFETY_MARGIN: u64 = 12;

impl OnRuntimeUpgrade for Migration {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		Ok(WitnessSafetyMargin::<Runtime, EthereumInstance>::get().encode())
	}

	fn on_runtime_upgrade() -> Weight {
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => {
				WitnessSafetyMargin::<Runtime, EthereumInstance>::put(NEW_ETH_SAFETY_MARGIN);
			},
			genesis_hashes::PERSEVERANCE => {
				// Nothing
			},
			genesis_hashes::SISYPHOS => {
				// Nothing
			},
			_ => {},
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let old_margin: Option<u64> = Decode::decode(&mut &state[..]).unwrap();
		let new_margin = WitnessSafetyMargin::<Runtime, EthereumInstance>::get();
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => assert_eq!(new_margin, Some(NEW_ETH_SAFETY_MARGIN)),
			genesis_hashes::PERSEVERANCE => {
				assert_eq!(new_margin, old_margin);
			},
			genesis_hashes::SISYPHOS => {
				assert_eq!(new_margin, old_margin);
			},
			_ => {},
		}
		Ok(())
	}
}
