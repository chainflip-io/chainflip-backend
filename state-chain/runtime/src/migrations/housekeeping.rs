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

use crate::{chainflip::SolEnvironment, Runtime};
use cf_chains::{assets, sol::SolAddress, AllBatch, ForeignChain, TransferAssetParams};
use cf_runtime_utilities::genesis_hashes;
use core::str::FromStr;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

pub mod liveness_election_state;
pub mod reap_old_accounts;
pub mod solana_remove_unused_channels_state;

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
			genesis_hashes::BERGHAIN => {
				if crate::VERSION.spec_version != 2_02_04 {
					log::info!("🧹 No housekeeping required for Berghain.");
					return Weight::zero();
				}

				// COM-113: refund a user who accidentally sent USDT (instead of USDC) into a
				// USDC->SOL swap. The deposit has been swept into the vault; refund it back to the
				// original sender.
				// Deposit tx: 2vnQfVVCtLxXu2E4S7pmqZvD7Nq296V3E8vV5xbWZ1ZxtxruAV31W55oKDaSBd9ev4qBtUNpLY4mEuR6jK6vWJ6J
				log::info!("🧹 Solana USDT refund for Berghain housekeeping...");
				// Amount: 10,000 USDT (10_000_000_000 base units, 6 dp)
				// Destination: CmAuZetSJA17ZGCo7L1bsKPTvY1fy48MFempvvRfCzGC (original sender)
				let Ok(mut res) = <cf_chains::sol::api::SolanaApi<SolEnvironment> as AllBatch<_>>::new_unsigned(
					Default::default(),
					(0..) // Dummy egress_ids: these aren't used.
						.zip([TransferAssetParams {
							asset: assets::sol::Asset::SolUsdt,
							amount: 10_000_000_000,
							to: SolAddress::from_str("CmAuZetSJA17ZGCo7L1bsKPTvY1fy48MFempvvRfCzGC")
								.expect("valid address; qed"),
						}])
						.map(|(a, b)| (b, (ForeignChain::Solana, a)))
						.collect(),
				) else {
					log::error!("Failed to construct Solana batch for Berghain housekeeping.");
					return Weight::zero();
				};
				let Some((api_call, _)) = res.pop() else {
					log::info!("Unexpected error.");
					return Weight::zero();
				};
				let _ = crate::SolanaBroadcaster::threshold_sign_and_broadcast(api_call);

				// Without doing this the events are cleared on_initialize and so
				// the engine will never see them.
				pallet_cf_cfe_interface::RuntimeUpgradeEvents::<Runtime>::put(
					pallet_cf_cfe_interface::CfeEvents::<Runtime>::take(),
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
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		Ok(())
	}
}
