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
