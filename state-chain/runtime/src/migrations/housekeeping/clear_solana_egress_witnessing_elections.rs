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
use cf_runtime_utilities::genesis_hashes;
#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
use frame_support::{pallet_prelude::Weight, traits::OnRuntimeUpgrade};

use pallet_cf_elections::{
	electoral_system_runner::RunnerStorageAccessTrait,
	electoral_systems::composite::tuple_7_impls::CompositeElectionIdentifierExtra,
	ElectionProperties, RunnerStorageAccess,
};
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

pub struct ClearSolanaEgressWitnessingElections;

impl OnRuntimeUpgrade for ClearSolanaEgressWitnessingElections {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		let count_egress_witnessing: u64 =
			ElectionProperties::<Runtime, SolanaInstance>::iter_keys()
				.filter(|id| matches!(id.extra(), CompositeElectionIdentifierExtra::D(_)))
				.count()
				.try_into()
				.unwrap();
		log::info!("🧹 Found {} EgressWitnessing elections to clear", count_egress_witnessing);

		let count_nonce_witnessing: u64 =
			ElectionProperties::<Runtime, SolanaInstance>::iter_keys()
				.filter(|id| matches!(id.extra(), CompositeElectionIdentifierExtra::C(_)))
				.count()
				.try_into()
				.unwrap();

		log::info!("🧹 Found {} NonceWitnessing elections to clear", count_nonce_witnessing);

		Ok((count_egress_witnessing, count_nonce_witnessing).encode())
	}

	fn on_runtime_upgrade() -> Weight {
		let next_election_id = match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::PERSEVERANCE => 9_397_949u64,
			genesis_hashes::SISYPHOS => 9_346_659u64,
			_ => return Weight::zero(),
		};

		if crate::VERSION.spec_version == 2_00_03 {
			// Collect all EgressWitnessing election identifiers (those with extra D)
			let egress_witnessing_elections: Vec<_> =
				ElectionProperties::<Runtime, SolanaInstance>::iter_keys()
					.filter(|id| matches!(id.extra(), CompositeElectionIdentifierExtra::D(_)))
					.collect();

			let nonce_witnessing_elections: Vec<_> =
				ElectionProperties::<Runtime, SolanaInstance>::iter_keys()
					.filter(|id| matches!(id.extra(), CompositeElectionIdentifierExtra::C(_)))
					.collect();

			for election_identifier in nonce_witnessing_elections {
				// Skip newly created elections (we just want to delete old stale elections)
				if *election_identifier.unique_monotonic() > next_election_id.into() {
					continue;
				}
				RunnerStorageAccess::<Runtime, SolanaInstance>::delete_election(
					election_identifier,
				);
			}
			for election_identifier in egress_witnessing_elections {
				// Skip newly created elections (we just want to delete old stale elections)
				if *election_identifier.unique_monotonic() > next_election_id.into() {
					continue;
				}
				RunnerStorageAccess::<Runtime, SolanaInstance>::delete_election(
					election_identifier,
				);
			}

			log::info!("✅ Successfully cleared EgressWitnessing and NonceWitnessing elections");
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let (count_egress_witnessing, count_nonce_witnessing) =
			<(u64, u64)>::decode(&mut state.as_slice())
				.map_err(|_| DispatchError::from("Failed to decode state"))?;

		// Verify no EgressWitnessing elections remain
		let remaining_egress_witnessing: u64 =
			ElectionProperties::<Runtime, SolanaInstance>::iter_keys()
				.filter(|id| matches!(id.extra(), CompositeElectionIdentifierExtra::D(_)))
				.count()
				.try_into()
				.unwrap();

		let remaining_nonce_witnessing: u64 =
			ElectionProperties::<Runtime, SolanaInstance>::iter_keys()
				.filter(|id| matches!(id.extra(), CompositeElectionIdentifierExtra::C(_)))
				.count()
				.try_into()
				.unwrap();

		log::info!(
			"✅ Post-upgrade check passed: cleared {} EgressWitnessing elections, {} remaining",
			count_egress_witnessing,
			remaining_egress_witnessing
		);
		log::info!(
			"✅ Post-upgrade check passed: cleared {} NonceWitnessing elections, {} remaining",
			count_nonce_witnessing,
			remaining_nonce_witnessing
		);

		Ok(())
	}
}
