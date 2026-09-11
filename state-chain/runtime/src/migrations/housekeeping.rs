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
use crate::{Runtime, VERSION};
#[cfg(feature = "try-runtime")]
use cf_chains::instances::{AssethubInstance, EthereumInstance, SolanaInstance, TronInstance};
use cf_runtime_utilities::genesis_hashes;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
#[cfg(feature = "try-runtime")]
use pallet_cf_ingress_egress::ScheduledEgressFetchOrTransfer;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

/// Pending egress queue lengths for every chain the refunds touch.
///
/// All three refund modules append to these queues, so the deltas can only be checked once they
/// have all run — an individual module cannot verify its own in isolation.
#[cfg(feature = "try-runtime")]
fn egress_queue_lengths() -> [u32; 4] {
	[
		ScheduledEgressFetchOrTransfer::<Runtime, EthereumInstance>::decode_len().unwrap_or(0)
			as u32,
		ScheduledEgressFetchOrTransfer::<Runtime, TronInstance>::decode_len().unwrap_or(0) as u32,
		ScheduledEgressFetchOrTransfer::<Runtime, SolanaInstance>::decode_len().unwrap_or(0) as u32,
		ScheduledEgressFetchOrTransfer::<Runtime, AssethubInstance>::decode_len().unwrap_or(0)
			as u32,
	]
}

/// What each chain's queue is expected to grow by, summed across the three modules.
#[cfg(feature = "try-runtime")]
const EXPECTED_EGRESS_DELTAS: [u32; 4] = [
	refunds::ETHEREUM_EGRESSES,
	refunds::TRON_EGRESSES + overcharged_gas::TRON_EGRESSES,
	stuck_channels::SOLANA_EGRESSES,
	stuck_channels::ASSETHUB_EGRESSES,
];

#[cfg(feature = "try-runtime")]
const CHAIN_NAMES: [&str; 4] = ["Ethereum", "Tron", "Solana", "Assethub"];

pub mod liveness_election_state;
pub mod overcharged_gas;
pub mod reap_old_accounts;
pub mod refunds;
pub mod solana_remove_unused_channels_state;
pub mod stuck_channels;

// One-shot gate for the batched refunds. Must equal the runtime's spec_version at the moment the
// migration ships, so a release that forgets to remove it cannot pay the refunds out twice. Update
// this in lock-step with VERSION.spec_version, and remove the constant along with the `refunds` and
// `stuck_channels` modules in the release that follows.
const REFUNDS_SPEC_VERSION: u32 = 2_02_13;

pub type Migration = (
	NetworkSpecificHousekeeping,
	reap_old_accounts::Migration,
	// Can be removed once Solana address re-use is activated.
	solana_remove_unused_channels_state::SolanaRemoveUnusedChannelsState,
	liveness_election_state::LivenessElectionStateMigration,
);

pub struct NetworkSpecificHousekeeping;

impl OnRuntimeUpgrade for NetworkSpecificHousekeeping {
	fn on_runtime_upgrade() -> Weight {
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN =>
				if VERSION.spec_version == REFUNDS_SPEC_VERSION {
					// Must run first: it credits the recovered channel balances that the refund
					// egresses below are paid out of.
					stuck_channels::Migration::on_runtime_upgrade();
					refunds::Migration::on_runtime_upgrade();
					overcharged_gas::Migration::on_runtime_upgrade();
					log::info!("🧹 Berghain: scheduled batched refunds.");
				} else {
					log::info!(
						"🧹 Skipping refunds: spec_version is {} (expected {}).",
						VERSION.spec_version,
						REFUNDS_SPEC_VERSION,
					);
				},
			genesis_hashes::PERSEVERANCE => {
				log::info!("🧹 No housekeeping required for Perseverance.");
			},
			genesis_hashes::SISYPHOS => {
				log::info!("🧹 No housekeeping required for Sisyphos.");
			},
			_ => {},
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		if matches!(genesis_hashes::genesis_hash::<Runtime>(), genesis_hashes::BERGHAIN) &&
			VERSION.spec_version == REFUNDS_SPEC_VERSION
		{
			Ok(egress_queue_lengths().iter().flat_map(|len| len.to_be_bytes()).collect())
		} else {
			Ok(Default::default())
		}
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		if !matches!(genesis_hashes::genesis_hash::<Runtime>(), genesis_hashes::BERGHAIN) ||
			VERSION.spec_version != REFUNDS_SPEC_VERSION
		{
			return Ok(());
		}

		if state.len() != 16 {
			return Err(DispatchError::Other("bad pre_upgrade state"));
		}

		let after = egress_queue_lengths();
		for (i, chunk) in state.chunks_exact(4).enumerate() {
			let before = u32::from_be_bytes(
				chunk.try_into().map_err(|_| DispatchError::Other("bad pre_upgrade state"))?,
			);
			assert_eq!(
				after[i],
				before + EXPECTED_EGRESS_DELTAS[i],
				"unexpected {} egress queue delta",
				CHAIN_NAMES[i],
			);
		}

		Ok(())
	}
}
