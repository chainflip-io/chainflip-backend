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

use crate::{chainflip::Offence, Runtime};
use cf_primitives::FLIPPERINOS_PER_FLIP;
use cf_runtime_utilities::genesis_hashes;
use cf_traits::AccountInfo;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use sp_runtime::AccountId32;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
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

const ACCOUNTS: [[u8; 32]; 3] = [
	hex_literal::hex!("02f18b9f9803d316012ed003d9390d489b3879007bb4834238216584b2fdea4e"),
	hex_literal::hex!("0e759566cd716d9b02e844a04339b68565af75b7aea07356c0a91fa1df80e052"),
	hex_literal::hex!("6e2fbd539aaff7648228a76a8c939e4b945c1d8a1f8a229165c7cd29963daa1c"),
];

pub struct NetworkSpecificHousekeeping;

impl OnRuntimeUpgrade for NetworkSpecificHousekeeping {
	fn on_runtime_upgrade() -> Weight {
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => {
				if crate::VERSION.spec_version == 2_02_12 {
					let account_ids: Vec<AccountId32> =
						ACCOUNTS.into_iter().map(AccountId32::from).collect();
					for offence in [
						// Prevents signing participation
						Offence::ParticipateKeygenFailed,
						// Prevents Keygen participation
						Offence::GrandpaEquivocation,
					] {
						pallet_cf_reputation::Pallet::<Runtime>::suspend_all(
							account_ids.clone(),
							&offence,
							u32::MAX,
						);
					}
					for account_id in account_ids {
						if !frame_system::Pallet::<Runtime>::account_exists(&account_id) {
							log::warn!(
								"🧹 Skipping housekeeping for non-existent account: {account_id:?}"
							);
							continue
						}
						let balance = pallet_cf_flip::Pallet::<Runtime>::balance(&account_id);
						pallet_cf_flip::Pallet::<Runtime>::settle(
							&account_id,
							pallet_cf_flip::Pallet::<Runtime>::deposit_reserves(
								*b"QUAR",
								balance.saturating_sub(FLIPPERINOS_PER_FLIP),
							)
							.into(),
						);
					}
				}
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
