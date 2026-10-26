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

//! The Assethub `ChainBlockHash` changed from the witness batch number to `H256`, which changes
//! the encoding of most of the Assethub elections storage. Assethub elections aren't live on
//! Berghain, so instead of translating the state we reset and re-initialize it on Perseverance and
//! Sisyphos.
//! Any Assethub blocks between the last witnessed block and the block the restarted BHW starts
//! from will not be witnessed.
//!
//! This isn't gated by a storage version, so it runs on every upgrade while it's included.

use crate::{chainflip::witnessing::assethub_elections, Runtime};
use cf_chains::instances::AssethubInstance;
use cf_runtime_utilities::genesis_hashes;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

pub struct ResetAssethubElections;

impl OnRuntimeUpgrade for ResetAssethubElections {
	fn on_runtime_upgrade() -> Weight {
		if matches!(
			genesis_hashes::genesis_hash::<Runtime>(),
			genesis_hashes::PERSEVERANCE | genesis_hashes::SISYPHOS
		) {
			pallet_cf_elections::Pallet::<Runtime, AssethubInstance>::reset();
			if let Err(err) =
				pallet_cf_elections::Pallet::<Runtime, AssethubInstance>::internally_initialize(
					assethub_elections::initial_state(),
				) {
				log::error!("Failed to re-initialize Assethub elections after reset: {err:?}");
			}
		} else {
			log::info!("Not on Perseverance or Sisyphos, skipping Assethub elections reset.");
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		use pallet_cf_elections::{
			ElectionProperties, ElectoralUnsynchronisedSettings, ElectoralUnsynchronisedState,
			SharedDataReferenceLifetime,
		};

		if matches!(
			genesis_hashes::genesis_hash::<Runtime>(),
			genesis_hashes::PERSEVERANCE | genesis_hashes::SISYPHOS
		) {
			let initial_state = assethub_elections::initial_state();

			frame_support::ensure!(
				ElectoralUnsynchronisedState::<Runtime, AssethubInstance>::get() ==
					Some(initial_state.unsynchronised_state),
				"Assethub elections unsynchronised state was not reset"
			);
			frame_support::ensure!(
				ElectoralUnsynchronisedSettings::<Runtime, AssethubInstance>::get() ==
					Some(initial_state.unsynchronised_settings),
				"Assethub elections unsynchronised settings were not reset"
			);
			frame_support::ensure!(
				SharedDataReferenceLifetime::<Runtime, AssethubInstance>::get() ==
					initial_state.shared_data_reference_lifetime,
				"Assethub elections shared data reference lifetime was not reset"
			);
			frame_support::ensure!(
				ElectionProperties::<Runtime, AssethubInstance>::iter_keys().next().is_none(),
				"Assethub elections still has elections"
			);
		}

		Ok(())
	}
}
