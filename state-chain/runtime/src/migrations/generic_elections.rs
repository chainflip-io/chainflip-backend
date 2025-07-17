use crate::{chainflip::generic_elections::ChainlinkOraclePriceSettings, *};
use frame_support::{pallet_prelude::Weight, traits::OnRuntimeUpgrade};

use crate::chainflip::generic_elections;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		Ok(().encode())
	}

	fn on_runtime_upgrade() -> Weight {
		let chainlink_oracle_price_settings =
			match cf_runtime_utilities::genesis_hashes::genesis_hash::<Runtime>() {
				cf_runtime_utilities::genesis_hashes::BERGHAIN => ChainlinkOraclePriceSettings {
					sol_oracle_program_id: todo!(),
					sol_oracle_feeds: todo!(),
					sol_oracle_query_helper: todo!(),
					eth_contract_address: todo!(),
					eth_oracle_feeds: todo!(),
				},
				cf_runtime_utilities::genesis_hashes::PERSEVERANCE =>
					ChainlinkOraclePriceSettings {
						sol_oracle_program_id: todo!(),
						sol_oracle_feeds: todo!(),
						sol_oracle_query_helper: todo!(),
						eth_contract_address: todo!(),
						eth_oracle_feeds: todo!(),
					},
				cf_runtime_utilities::genesis_hashes::SISYPHOS => ChainlinkOraclePriceSettings {
					sol_oracle_program_id: todo!(),
					sol_oracle_feeds: todo!(),
					sol_oracle_query_helper: todo!(),
					eth_contract_address: todo!(),
					eth_oracle_feeds: todo!(),
				},
				// localnet:
				_ => ChainlinkOraclePriceSettings {
					sol_oracle_program_id: todo!(),
					sol_oracle_feeds: todo!(),
					sol_oracle_query_helper: todo!(),
					eth_contract_address: todo!(),
					eth_oracle_feeds: todo!(),
				},
			};

		let _result = pallet_cf_elections::Pallet::<Runtime, ()>::internally_initialize(
			generic_elections::initial_state(chainlink_oracle_price_settings),
		);
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		use pallet_cf_elections::SharedDataReferenceLifetime;

		let lifetime = SharedDataReferenceLifetime::<Runtime, ()>::get();
		assert_eq!(lifetime, 8);
		Ok(())
	}
}
