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
use cf_chains::{
	btc::{BitcoinNetwork, ScriptPubkey},
	instances::BitcoinInstance,
};
use cf_runtime_utilities::genesis_hashes;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use pallet_cf_ingress_egress::FetchOrTransfer;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

pub mod egresses;
pub mod liveness_election_state;
pub mod reap_old_accounts;
pub mod solana_remove_unused_channels_state;

// One-shot gate for the refund_stuck_funds migration. Must equal the runtime's
// spec_version at the moment the migration ships. If a future release forgets
// to remove the migration, the version mismatch prevents a re-run. Update this
// in lock-step with VERSION.spec_version, and remove both the constant and the
// refund_stuck_funds module in the release that follows.
const REFUND_STUCK_FUNDS_SPEC_VERSION: u32 = 2_01_14;

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
				if VERSION.spec_version == REFUND_STUCK_FUNDS_SPEC_VERSION && pallet_cf_ingress_egress::ScheduledEgressFetchOrTransfer::<Runtime, BitcoinInstance>::get().into_iter().find(|item|
					matches!(
						item,
						FetchOrTransfer::Transfer { destination_address, .. }
							if (destination_address == &ScriptPubkey::try_from_address("bc1qgqez4lvdm8xcgj3yyqjlygqu26ggdxqcc69p0e", &BitcoinNetwork::Mainnet).expect("address is valid")))
				).is_none() {
					egresses::Migration::on_runtime_upgrade();
					log::info!(
						"🧹 Berghain: scheduled refunds for stuck BTC, ETH, USDT and USDC deposits."
					);
				} else {
					log::info!(
						"🧹 Skipping refund_stuck_funds: spec_version is {} (expected {}).",
						VERSION.spec_version,
						REFUND_STUCK_FUNDS_SPEC_VERSION,
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
			VERSION.spec_version == REFUND_STUCK_FUNDS_SPEC_VERSION
		{
			egresses::Migration::pre_upgrade()
		} else {
			Ok(Default::default())
		}
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		if matches!(genesis_hashes::genesis_hash::<Runtime>(), genesis_hashes::BERGHAIN) &&
			VERSION.spec_version == REFUND_STUCK_FUNDS_SPEC_VERSION
		{
			egresses::Migration::post_upgrade(state)?;
		}
		Ok(())
	}
}
