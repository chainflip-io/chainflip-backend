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

use crate::*;
#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
use frame_support::{pallet_prelude::Weight, traits::OnRuntimeUpgrade};

use pallet_cf_elections::{
	electoral_systems::composite::tuple_7_impls::CompositeElectionIdentifierExtra,
	BitmapComponents, ElectionConsensusHistory, ElectionConsensusHistoryUpToDate,
	ElectionProperties, ElectionState,
};
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

pub struct ClearSolanaEgressWitnessingElections;

impl OnRuntimeUpgrade for ClearSolanaEgressWitnessingElections {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		let count: u64 = ElectionProperties::<Runtime, SolanaInstance>::iter_keys()
			.filter(|id| matches!(id.extra(), CompositeElectionIdentifierExtra::D(_)))
			.count()
			.try_into()
			.unwrap();
		log::info!("🧹 Found {} EgressWitnessing elections to clear", count);
		Ok(count.encode())
	}

	fn on_runtime_upgrade() -> Weight {
		// Collect all EgressWitnessing election identifiers (those with extra D)
		let egress_witnessing_elections: Vec<_> =
			ElectionProperties::<Runtime, SolanaInstance>::iter_keys()
				.filter(|id| matches!(id.extra(), CompositeElectionIdentifierExtra::D(_)))
				.collect();

		for election_identifier in egress_witnessing_elections {
			let unique_monotonic_identifier = *election_identifier.unique_monotonic();

			// Clear vote-related storage for this election
			BitmapComponents::<Runtime, SolanaInstance>::remove(unique_monotonic_identifier);

			// Remove election properties and state
			ElectionProperties::<Runtime, SolanaInstance>::remove(election_identifier);
			ElectionState::<Runtime, SolanaInstance>::remove(unique_monotonic_identifier);

			// Remove consensus-related storage
			ElectionConsensusHistory::<Runtime, SolanaInstance>::remove(
				unique_monotonic_identifier,
			);
			ElectionConsensusHistoryUpToDate::<Runtime, SolanaInstance>::remove(
				unique_monotonic_identifier,
			);
		}

		log::info!("✅ Successfully cleared EgressWitnessing elections");

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let count_before = <u64>::decode(&mut state.as_slice())
			.map_err(|_| DispatchError::from("Failed to decode state"))?;

		// Verify no EgressWitnessing elections remain
		let remaining: u64 = ElectionProperties::<Runtime, SolanaInstance>::iter_keys()
			.filter(|id| matches!(id.extra(), CompositeElectionIdentifierExtra::D(_)))
			.count()
			.try_into()
			.unwrap();

		if remaining != 0 {
			return Err(DispatchError::from("Some EgressWitnessing elections were not cleared"));
		}

		log::info!(
			"✅ Post-upgrade check passed: cleared {} EgressWitnessing elections, {} remaining",
			count_before,
			remaining
		);

		Ok(())
	}
}
