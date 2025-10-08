// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use crate::{chainflip::generic_elections::ChainlinkOraclePriceSettings, *};
use frame_support::{pallet_prelude::Weight, traits::OnRuntimeUpgrade};
use pallet_cf_elections::CorruptStorageAdherance;

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
		// we have to initialize the elections pallet if we're upgrading from version 1.10.x (this
		// only happens in the upgrade-test in CI).
		if pallet_cf_elections::Status::<Runtime, ()>::get().is_none() {
			let chainlink_oracle_price_settings = ChainlinkOraclePriceSettings {
				arb_address_checker: hex_literal::hex!("9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0")
					.into(),
				arb_oracle_feeds: vec![
					hex_literal::hex!("a85233C63b9Ee964Add6F2cffe00Fd84eb32338f").into(),
					hex_literal::hex!("4A679253410272dd5232B3Ff7cF5dbB88f295319").into(),
					hex_literal::hex!("7a2088a1bFc9d81c55368AE168C2C02570cB814F").into(),
					hex_literal::hex!("09635F643e140090A9A8Dcd712eD6285858ceBef").into(),
					hex_literal::hex!("c5a5C42992dECbae36851359345FE25997F5C42d").into(),
				],
				eth_address_checker: hex_literal::hex!("e7f1725E7734CE288F8367e1Bb143E90bb3F0512")
					.into(),
				eth_oracle_feeds: vec![
					hex_literal::hex!("322813Fd9A801c5507c9de605d63CEA4f2CE6c44").into(),
					hex_literal::hex!("a85233C63b9Ee964Add6F2cffe00Fd84eb32338f").into(),
					hex_literal::hex!("4A679253410272dd5232B3Ff7cF5dbB88f295319").into(),
					hex_literal::hex!("7a2088a1bFc9d81c55368AE168C2C02570cB814F").into(),
					hex_literal::hex!("09635F643e140090A9A8Dcd712eD6285858ceBef").into(),
				],
			};

			let _result = pallet_cf_elections::Pallet::<Runtime, ()>::internally_initialize(
				generic_elections::initial_state(chainlink_oracle_price_settings),
			);
		}

		if crate::VERSION.spec_version == 1_11_11 {
			let result = pallet_cf_elections::Pallet::<Runtime, ()>::update_settings(
				pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
				Some(
					generic_elections::initial_state(
						// It's not important what we pass in here, since we only want to extract
						// the unsychronized settings
						ChainlinkOraclePriceSettings {
							arb_address_checker: Default::default(),
							arb_oracle_feeds: Default::default(),
							eth_address_checker: Default::default(),
							eth_oracle_feeds: Default::default(),
						},
					)
					.unsynchronised_settings,
				),
				None,
				CorruptStorageAdherance::Heed,
			);

			match result {
				Ok(()) => log::info!("successfully updated price oracle election settings"),
				Err(err) =>
					log::error!("error when updating price oracle election settings: {err:?}"),
			}
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		Ok(())
	}
}
