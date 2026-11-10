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

//! Deletes in-flight liveness elections during runtime upgrades so they are recreated using the
//! updated runtime state. This avoids races between an upgrade and an ongoing liveness check.

use crate::Runtime;
use cf_chains::instances::{
	ArbitrumInstance, AssethubInstance, BitcoinInstance, BscInstance, EthereumInstance,
	SolanaInstance, TronInstance,
};
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use pallet_cf_elections::{
	electoral_system_runner::RunnerStorageAccessTrait,
	electoral_systems::composite::{
		tuple_5_impls::CompositeElectionIdentifierExtra as Tuple5Extra,
		tuple_6_impls::CompositeElectionIdentifierExtra as Tuple6Extra,
		tuple_7_impls::CompositeElectionIdentifierExtra as Tuple7Extra,
		tuple_8_impls::CompositeElectionIdentifierExtra as Tuple8Extra,
	},
	ElectionProperties, RunnerStorageAccess,
};
use sp_std::vec::Vec;

pub struct LivenessElectionStateMigration;

impl OnRuntimeUpgrade for LivenessElectionStateMigration {
	fn on_runtime_upgrade() -> Weight {
		log::info!("🔄 Running liveness election state migration...");

		// Ethereum: FF variant is EthereumLiveness (6th in 8-tuple)
		let eth_elections: Vec<_> =
			ElectionProperties::<Runtime, EthereumInstance>::iter_keys().collect();
		for election_id in eth_elections {
			if matches!(election_id.extra(), Tuple8Extra::FF(_)) {
				RunnerStorageAccess::<Runtime, EthereumInstance>::delete_election(election_id);
			}
		}
		log::info!("🔄 Deleted Ethereum liveness election");

		// Arbitrum: FF variant is ArbitrumLiveness (6th in 6-tuple)
		let arb_elections: Vec<_> =
			ElectionProperties::<Runtime, ArbitrumInstance>::iter_keys().collect();
		for election_id in arb_elections {
			if matches!(election_id.extra(), Tuple6Extra::FF(_)) {
				RunnerStorageAccess::<Runtime, ArbitrumInstance>::delete_election(election_id);
			}
		}
		log::info!("🔄 Deleted Arbitrum liveness election");

		// Bitcoin: FF variant is BitcoinLiveness (6th in 6-tuple)
		let btc_elections: Vec<_> =
			ElectionProperties::<Runtime, BitcoinInstance>::iter_keys().collect();
		for election_id in btc_elections {
			if matches!(election_id.extra(), Tuple6Extra::FF(_)) {
				RunnerStorageAccess::<Runtime, BitcoinInstance>::delete_election(election_id);
			}
		}
		log::info!("🔄 Deleted Bitcoin liveness election");

		// Solana: EE variant is SolanaLiveness (5th in 7-tuple)
		let sol_elections: Vec<_> =
			ElectionProperties::<Runtime, SolanaInstance>::iter_keys().collect();
		for election_id in sol_elections {
			if matches!(election_id.extra(), Tuple7Extra::EE(_)) {
				RunnerStorageAccess::<Runtime, SolanaInstance>::delete_election(election_id);
			}
		}
		log::info!("🔄 Deleted Solana liveness election");

		// Assethub: EE variant is AssethubLiveness (5th in 5-tuple)
		let assethub_elections: Vec<_> =
			ElectionProperties::<Runtime, AssethubInstance>::iter_keys().collect();
		for election_id in assethub_elections {
			if matches!(election_id.extra(), Tuple5Extra::EE(_)) {
				RunnerStorageAccess::<Runtime, AssethubInstance>::delete_election(election_id);
			}
		}
		log::info!("🔄 Deleted Assethub liveness election");

		// Tron: EE variant is TronLiveness (5th in 5-tuple)
		let tron_elections: Vec<_> =
			ElectionProperties::<Runtime, TronInstance>::iter_keys().collect();
		for election_id in tron_elections {
			if matches!(election_id.extra(), Tuple5Extra::EE(_)) {
				RunnerStorageAccess::<Runtime, TronInstance>::delete_election(election_id);
			}
		}
		log::info!("🔄 Deleted Tron liveness election");

		// BSC: FF variant is BscLiveness (6th in 6-tuple)
		let bsc_elections: Vec<_> =
			ElectionProperties::<Runtime, BscInstance>::iter_keys().collect();
		for election_id in bsc_elections {
			if matches!(election_id.extra(), Tuple6Extra::FF(_)) {
				RunnerStorageAccess::<Runtime, BscInstance>::delete_election(election_id);
			}
		}
		log::info!("🔄 Deleted BSC liveness election");

		Weight::zero()
	}
}
