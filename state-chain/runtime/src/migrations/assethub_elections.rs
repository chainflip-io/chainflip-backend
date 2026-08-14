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

use crate::Runtime;
use cf_chains::instances::AssethubInstance;
#[cfg(feature = "try-runtime")]
use codec::Encode;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

pub struct AssethubElectionsInit;

impl OnRuntimeUpgrade for AssethubElectionsInit {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(().encode())
	}

	fn on_runtime_upgrade() -> Weight {
		let result =
			pallet_cf_elections::Pallet::<Runtime, AssethubInstance>::internally_initialize(
				crate::chainflip::witnessing::assethub_elections::initial_state(),
			);
		if result.is_err() {
			log::info!("Assethub Elections already initialised.");
		}
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		use pallet_cf_elections::{ElectoralUnsynchronisedSettings, SharedDataReferenceLifetime};

		let initial_state = crate::chainflip::witnessing::assethub_elections::initial_state();

		assert_eq!(
			ElectoralUnsynchronisedSettings::<Runtime, AssethubInstance>::get(),
			Some(initial_state.unsynchronised_settings)
		);
		assert_eq!(
			SharedDataReferenceLifetime::<Runtime, AssethubInstance>::get(),
			initial_state.shared_data_reference_lifetime
		);

		Ok(())
	}
}
