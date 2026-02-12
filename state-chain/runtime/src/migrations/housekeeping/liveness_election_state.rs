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

//! Migration for Liveness ElectionState change.
//!
//! The Liveness electoral system's ElectionState changed from StateChainBlockNumber
//! to (StateChainBlockNumber, EpochIndex). This migration deletes existing liveness
//! elections so they get recreated with the new state format.

use crate::Runtime;
use cf_chains::instances::{BitcoinInstance, SolanaInstance};
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use pallet_cf_elections::{
	electoral_system_runner::RunnerStorageAccessTrait,
	electoral_systems::composite::{
		tuple_6_impls::CompositeElectionIdentifierExtra as BtcExtra,
		tuple_7_impls::CompositeElectionIdentifierExtra as SolExtra,
	},
	ElectionProperties, RunnerStorageAccess,
};
use sp_std::vec::Vec;

pub struct LivenessElectionStateMigration;

impl OnRuntimeUpgrade for LivenessElectionStateMigration {
	fn on_runtime_upgrade() -> Weight {
		log::info!("🔄 Running liveness election state migration...");

		// Bitcoin: FF variant is BitcoinLiveness (6th in 6-tuple)
		let btc_elections: Vec<_> =
			ElectionProperties::<Runtime, BitcoinInstance>::iter_keys().collect();
		for election_id in btc_elections {
			if matches!(election_id.extra(), BtcExtra::FF(_)) {
				RunnerStorageAccess::<Runtime, BitcoinInstance>::delete_election(election_id);
			}
		}
		log::info!("🔄 Deleted Bitcoin liveness election");

		// Solana: EE variant is SolanaLiveness (5th in 7-tuple)
		let sol_elections: Vec<_> =
			ElectionProperties::<Runtime, SolanaInstance>::iter_keys().collect();
		for election_id in sol_elections {
			if matches!(election_id.extra(), SolExtra::EE(_)) {
				RunnerStorageAccess::<Runtime, SolanaInstance>::delete_election(election_id);
			}
		}
		log::info!("🔄 Deleted Solana liveness election");

		Weight::zero()
	}
}
