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

use crate::{BitcoinBroadcaster, BitcoinChainTracking, BitcoinThresholdSigner, Runtime};
use cf_chains::{
	btc::{
		api::{batch_transfer::BatchTransfer, BitcoinApi},
		deposit_address::DepositAddress,
		BitcoinOutput, ScriptPubkey, Utxo, UtxoId, BITCOIN_DUST_LIMIT,
	},
	FeeEstimationApi,
};
use cf_primitives::chains::assets::btc::Asset as BtcAsset;
use cf_runtime_utilities::genesis_hashes;
use cf_traits::KeyProvider;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use sp_core::H256;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

pub mod reap_old_accounts;
pub mod solana_remove_unused_channels_state;

pub type Migration = (
	NetworkSpecificHousekeeping,
	reap_old_accounts::Migration,
	// Can be removed once Solana address re-use is activated.
	solana_remove_unused_channels_state::SolanaRemoveUnusedChannelsState,
);

const CHANNEL_ID: u32 = 111_157;
const SATS_AMOUNT: u64 = 3000000;

// See DepositBoosted in block https://scan.chainflip.io/blocks/9747296
fn utxos() -> [Utxo; 1] {
	[Utxo {
		id: UtxoId {
			// 6a2887dbf9fa616c5a3cd04b5b5ac60d93dfb9b8e6dd1c686b7621291bab72c5 reversed byte
			// order
			tx_id: H256(hex_literal::hex!(
				"c572ab1b2921766b681cdde6b8b9df930dc65a5b4bd03c5a6c61faf9db87286a"
			)),
			vout: 1,
		},
		amount: SATS_AMOUNT,
		deposit_address: DepositAddress::new(
			// Vault pubkey.
			hex_literal::hex!("083b85c09bbf3a13f9085d9a5c8db5c11c517df70c224ca808399899ff6f5db0"),
			// Channel ID.
			CHANNEL_ID,
		),
	}]
}

pub struct NetworkSpecificHousekeeping;

impl OnRuntimeUpgrade for NetworkSpecificHousekeeping {
	fn on_runtime_upgrade() -> Weight {
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => {
				let utxos = utxos();
				if crate::VERSION.spec_version == 1_10_06 {
					use pallet_cf_cfe_interface::{CfeEvents, RuntimeUpgradeEvents};
					// Clear out any old events.
					CfeEvents::<Runtime>::kill();
					RuntimeUpgradeEvents::<Runtime>::kill();

					log::info!("🧹 Doing housekeeping.");
					let fee_estimator = BitcoinChainTracking::chain_state()
						.expect("Chain state always exists")
						.tracked_data;
					let ingress_fee = fee_estimator
						.estimate_ingress_fee_vault_swap()
						.expect("always returns Some");
					let egress_fee = fee_estimator.estimate_egress_fee(BtcAsset::Btc);
					let fee_estimate = ingress_fee + egress_fee; // 1 input, 1 output
					if SATS_AMOUNT - fee_estimate < BITCOIN_DUST_LIMIT {
						log::warn!("❗️ Fees too high!");
					}
					let agg_key = BitcoinThresholdSigner::active_epoch_key()
						.expect("Current key always exists")
						.key;
					let (broadcast_id, threshold_id) =
						BitcoinBroadcaster::threshold_sign_and_broadcast(
							BitcoinApi::BatchTransfer(BatchTransfer::new_unsigned(
								&agg_key,
								agg_key.current,
								// Inputs:
								utxos.into_iter().collect(),
								// Outputs:
								[
									BitcoinOutput {
										amount: SATS_AMOUNT.saturating_sub(fee_estimate),
										script_pubkey: ScriptPubkey::try_from_address(
											"bc1pdcgke78tp9gp2869ekw38dd2cuv3zfuy3lz605uez57rtan7w0ps92u3gm",
											&cf_chains::btc::BitcoinNetwork::Mainnet,
										)
										.expect("Valid address"),
									},
								]
								.into_iter()
								.filter(|o| o.amount >= BITCOIN_DUST_LIMIT)
								.collect(),
							)),
							None,
							|_| None,
						);

					// Move events into the runtime upgrade events.
					RuntimeUpgradeEvents::<Runtime>::put(CfeEvents::<Runtime>::take());
					log::info!("🧹 Requested signature and broadcast with IDs {broadcast_id}:{threshold_id}.")
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
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		use cf_chains::instances::BitcoinInstance;
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => {
				let id_counter =
					pallet_cf_broadcast::BroadcastIdCounter::<Runtime, BitcoinInstance>::get();
				Ok(id_counter.to_le_bytes().to_vec())
			},
			_ => Ok(Default::default()),
		}
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		use cf_chains::instances::BitcoinInstance;
		#[allow(clippy::single_match)]
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN =>
				if crate::VERSION.spec_version == 1_10_06 {
					let old_id_counter = u32::from_le_bytes(
						state
							.try_into()
							.map_err(|_| DispatchError::Other("Invalid pre-upgrade state"))?,
					);
					let new_id_counter =
						pallet_cf_broadcast::BroadcastIdCounter::<Runtime, BitcoinInstance>::get();
					assert!(
						new_id_counter - old_id_counter == 1,
						"Expected exactly one new broadcast",
					);
					assert!(
						pallet_cf_broadcast::PendingBroadcasts::<Runtime, BitcoinInstance>::get()
							.contains(&new_id_counter),
						"New broadcast should be pending"
					);
				},
			_ => {},
		}

		Ok(())
	}
}
