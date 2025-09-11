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

// Broker channel for `cFKpid38PmmZ8V81AHaZAhHzzpRbsf7Xw5PYt5ajTXAUvHoTQ`
const CHANNEL_ID: u32 = 95_865;

fn utxos() -> [Utxo; 3] {
	[
		Utxo {
			id: UtxoId {
				// 5db81136442f77337b61b73ea138bfc4a746d40b374178f3555df63db74e1b32 reversed byte
				// order
				tx_id: H256(hex_literal::hex!(
					"321b4eb73df65d55f37841370bd446a7c4bf38a13eb7617b33772f443611b85d"
				)),
				vout: 0,
			},
			amount: 4539_6000, // 0.4539600 BTC
			deposit_address: DepositAddress::new(
				// Vault pubkey.
				hex_literal::hex!(
					"d3b352e8e2ac14fc48eda20dc64e9b1a2ca763620507a1b6884a917cca6a8361"
				),
				// Channel ID.
				CHANNEL_ID,
			),
		},
		Utxo {
			id: UtxoId {
				// 721448edccb3270f0581a7dcfd94408ab101a93e09678b18f8d8849d0cd8f4fd reversed byte
				// order
				tx_id: H256(hex_literal::hex!(
					"fdf4d80c9d84d8f8188b67093ea901b18a4094fddca781050f27b3cced481472"
				)),
				vout: 0,
			},
			amount: 4557_2000, // 0.4557200 BTC
			deposit_address: DepositAddress::new(
				// Vault pubkey.
				hex_literal::hex!(
					"d3b352e8e2ac14fc48eda20dc64e9b1a2ca763620507a1b6884a917cca6a8361"
				),
				// Channel ID.
				CHANNEL_ID,
			),
		},
		Utxo {
			id: UtxoId {
				// f93f4fc4a32c5493ee36e22430cb3496f2821930e9a405aa1bb98c7c7f94bd9b reversed byte
				// order
				tx_id: H256(hex_literal::hex!(
					"9bbd947f7c8cb91baa05a4e9301982f29634cb3024e236ee93542ca3c44f3ff9"
				)),
				vout: 0,
			},
			amount: 105_0000, // 0.0105000 BTC
			deposit_address: DepositAddress::new(
				// Previous vault pubkey.
				hex_literal::hex!(
					"0e7ec409bcd1f7e9bd6b6a7adb894ab634d2f792914793e4755837cfd8a3866e"
				),
				// Channel ID.
				CHANNEL_ID,
			),
		},
	]
}

pub struct NetworkSpecificHousekeeping;

impl OnRuntimeUpgrade for NetworkSpecificHousekeeping {
	fn on_runtime_upgrade() -> Weight {
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => {
				let utxos = utxos();
				if crate::VERSION.spec_version == 1_10_05 {
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
					let fee_estimate_1 = ingress_fee * 2 + egress_fee; // 2 inputs, 1 output
					let fee_estimate_2 = ingress_fee + egress_fee; // 1 input, 1 output
					if 105_0000 - fee_estimate_2 < BITCOIN_DUST_LIMIT {
						log::error!("🧹 Skipping tx 2: fees too high.");
					}
					let agg_key = BitcoinThresholdSigner::active_epoch_key()
						.expect("Current key always exists")
						.key;
					let (broadcast_id, threshold_id) =
						BitcoinBroadcaster::threshold_sign_and_broadcast(
							BitcoinApi::BatchTransfer(BatchTransfer::new_unsigned(
								&agg_key,
								agg_key.current,
								utxos.into_iter().collect(),
								[
									BitcoinOutput {
										amount: (4539_6000u64 + 4557_2000u64)
											.saturating_sub(fee_estimate_1),
										script_pubkey: ScriptPubkey::try_from_address(
											"bc1q9hkpvmzefgpr96r8920rp4rwm68g994tmpg2je",
											&cf_chains::btc::BitcoinNetwork::Mainnet,
										)
										.expect("Valid address"),
									},
									BitcoinOutput {
										amount: 105_0000u64.saturating_sub(fee_estimate_2),
										script_pubkey: ScriptPubkey::try_from_address(
											"bc1qjs0lr34ay6h7u7yveeg8q5qvufhq88thun9pu4",
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
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN =>
				if crate::VERSION.spec_version == 1_10_05 {
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
