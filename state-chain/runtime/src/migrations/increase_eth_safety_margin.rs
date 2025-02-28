use crate::Runtime;
use cf_chains::instances::{ArbitrumInstance, EthereumInstance};
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

// Keeping it a multiple of 24 to match the witness period of Arbitrum
const NEW_ARB_SAFETY_MARGIN: u64 = 672;

impl OnRuntimeUpgrade for Migration {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		use cf_chains::instances::ArbitrumInstance;

		let eth_margin = WitnessSafetyMargin::<Runtime, EthereumInstance>::get();
		let arb_margin = WitnessSafetyMargin::<Runtime, ArbitrumInstance>::get();

		let mut encoded = eth_margin.encode();
		encoded.extend(arb_margin.encode());

		Ok(encoded)
	}

	fn on_runtime_upgrade() -> Weight {
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => {
				WitnessSafetyMargin::<Runtime, EthereumInstance>::put(NEW_ETH_SAFETY_MARGIN);
				WitnessSafetyMargin::<Runtime, ArbitrumInstance>::put(NEW_ARB_SAFETY_MARGIN);
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
		let (old_eth_margin, old_arb_margin): (Option<u64>, Option<u64>) =
			Decode::decode(&mut &state[..])
				.map_err(|_| DispatchError::Other("Failed to decode state"))?;

		let new_eth_margin = WitnessSafetyMargin::<Runtime, EthereumInstance>::get();
		let new_arb_margin = WitnessSafetyMargin::<Runtime, ArbitrumInstance>::get();
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => {
				assert_eq!(new_eth_margin, Some(NEW_ETH_SAFETY_MARGIN));
				assert_eq!(new_arb_margin, Some(NEW_ARB_SAFETY_MARGIN));
			},
			genesis_hashes::PERSEVERANCE => {
				assert_eq!(new_eth_margin, old_eth_margin);
				assert_eq!(new_arb_margin, old_arb_margin);
			},
			genesis_hashes::SISYPHOS => {
				assert_eq!(new_eth_margin, old_eth_margin);
				assert_eq!(new_arb_margin, old_arb_margin);
			},
			_ => {},
		}
		Ok(())
	}
}
