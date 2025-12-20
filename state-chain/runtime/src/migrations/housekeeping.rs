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

use crate::Runtime;
use cf_chains::{
	assets,
	btc::{
		api::{batch_transfer::BatchTransfer, BitcoinApi},
		deposit_address::DepositAddress,
		BitcoinOutput, BitcoinTransaction, Utxo, UtxoId, CHANGE_ADDRESS_SALT,
	},
	instances::BitcoinInstance,
};
use cf_runtime_utilities::genesis_hashes;
use sp_std::collections::btree_set::BTreeSet;

use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
use sp_std::vec::Vec;

pub mod clear_solana_egress_witnessing_elections;
pub mod reap_old_accounts;
pub mod solana_remove_unused_channels_state;

pub type Migration = (
	NetworkSpecificHousekeeping,
	reap_old_accounts::Migration,
	// Can be removed once Solana address re-use is activated.
	solana_remove_unused_channels_state::SolanaRemoveUnusedChannelsState,
	clear_solana_egress_witnessing_elections::ClearSolanaEgressWitnessingElections,
);

pub struct NetworkSpecificHousekeeping;

impl OnRuntimeUpgrade for NetworkSpecificHousekeeping {
	fn on_runtime_upgrade() -> Weight {
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => {
				if crate::VERSION.spec_version != 2_00_04 {
					log::info!("🧹 No housekeeping required for Berghain.");
					return Weight::zero();
				}
				// This is the current aggkey for Berghain:
				let change_key = hex_literal::hex!(
					"a18a328f6736e1f6d9f5d65dca812778a84e21d7a4d6816a6aa4d2b39d0632dd"
				);
				let vault_deposit_address = DepositAddress::new(change_key, CHANGE_ADDRESS_SALT);
				let bitcoin_change_script = vault_deposit_address.script_pubkey();

				let available_utxos =
					pallet_cf_environment::BitcoinAvailableUtxos::<Runtime>::get();

				let mut pending_tx_ids = Vec::new();
				let mut all_internal_outputs = BTreeSet::new();
				let mut all_external_outputs = BTreeSet::new();
				let mut duplicate_value = 0u64;
				let mut all_internal_inputs = BTreeSet::new();
				let mut all_external_inputs = BTreeSet::new();
				let mut broadcast_ids = Vec::new();

				let spent_utxos = [
					Utxo {
						id: UtxoId {
							tx_id: hex_literal::hex!(
								"be88e3bc6683cbefccf2c5eed8e5147e48a9764fd9948336b37f1fa9115b6150"
							)
							.into(),
							vout: 0,
						},
						amount: 139_751,
						deposit_address: DepositAddress::new(change_key, 95_865),
					},
					Utxo {
						id: UtxoId {
							tx_id: hex_literal::hex!(
								"87fec35345d13aa816a7486a3203a575c53e6118bfad8c08e7f6d81661d761c7"
							)
							.into(),
							vout: 0,
						},
						amount: 314_446,
						deposit_address: DepositAddress::new(change_key, 95_865),
					},
				];
				let unspendable_utxo_ids = [
					UtxoId {
						tx_id: hex_literal::hex!(
							"6f0cd5a250b3263385c3b055062e0693a056bc8735a0b910870d4bdc8fea85be"
						)
						.into(),
						vout: 5,
					},
					UtxoId {
						tx_id: hex_literal::hex!(
							"1e77e8671d5df15bb09cce3402a16367dc798905300579138b705a3565172017"
						)
						.into(),
						vout: 5,
					},
				];

				for (broadcast_id, api_call) in
					pallet_cf_broadcast::PendingApiCalls::<Runtime, BitcoinInstance>::iter()
				{
					broadcast_ids.push(broadcast_id);
					match api_call {
						BitcoinApi::BatchTransfer(BatchTransfer {
							bitcoin_transaction:
								ref bitcoin_transaction @ BitcoinTransaction {
									ref inputs,
									ref outputs,
									signer_and_signatures: _,
									transaction_bytes: _,
									old_utxo_input_indices: _,
								},
							change_utxo_key,
						}) => {
							if change_utxo_key != change_key {
								log::warn!("Unexpected change_utxo_key found in pending broadcasts. Broadcast ID: {:?}, change_utxo_key: {:x?}", broadcast_id, change_utxo_key);
							}
							let tx_id = bitcoin_transaction.txid();
							pending_tx_ids.push(tx_id);
							if available_utxos.iter().any(|utxo| utxo.id.tx_id == tx_id) {
								log::info!("Transaction {tx_id} is referenced in available UTXOs.");
							}
							let (change_outputs, non_change_outputs): (BTreeSet<_>, BTreeSet<_>) =
								outputs.iter().cloned().enumerate().partition(|(_vout, output)| {
									output.script_pubkey == bitcoin_change_script
								});
							all_internal_outputs.extend(change_outputs.into_iter().map(
								|(vout, output)| {
									Utxo {
										id: UtxoId { tx_id, vout: vout as u32 },
										amount: output.amount,
										// We know this because it was filtered above.
										deposit_address: vault_deposit_address.clone(),
									}
								},
							));
							all_external_outputs.extend(non_change_outputs);

							let duplicate_input_count = {
								let mut seen = sp_std::collections::btree_set::BTreeSet::new();
								let mut duplicate_count = 0u32;
								for input in inputs {
									if !seen.insert(input.id.clone()) {
										log::info!(
											"Found duplicate input reference, value: {}",
											input.amount
										);
										duplicate_value += input.amount;
										duplicate_count += 1;
									}
								}
								duplicate_count
							};
							if duplicate_input_count > 0 {
								log::info!(
									"Transaction {tx_id} has {duplicate_input_count} duplicate input references."
								);
							}
							let (internal_inputs, external_inputs): (BTreeSet<_>, BTreeSet<_>) =
								inputs.iter().cloned().partition(|input| {
									input.deposit_address.script_pubkey() == bitcoin_change_script
								});
							all_internal_inputs.extend(internal_inputs);
							all_external_inputs.extend(external_inputs);
						},
						BitcoinApi::NoChangeTransfer(_) => {
							log::warn!("Unexpected NoChangeTransfer api call found in pending broadcasts. Broadcast ID: {:?}", broadcast_id);
						},
						_ => unreachable!(),
					}
				}

				let mut unspent_internal_outputs = 0u32;
				for output in &all_internal_outputs {
					if all_internal_inputs.remove(output) {
						log::info!(
							"Internal input is one of our own invalid outputs. Removing {}-{}.",
							output.id.tx_id,
							output.id.vout
						);
					} else {
						unspent_internal_outputs += 1;
						log::info!(
							"Internal output is unspent: {}-{}.",
							output.id.tx_id,
							output.id.vout
						);
						if pallet_cf_environment::BitcoinAvailableUtxos::<Runtime>::get()
							.contains(output)
						{
							log::info!(
								"Unspent internal output {}-{} is in available UTXOs and will be removed.",
								output.id.tx_id,
								output.id.vout
							);
						} else {
							log::warn!(
								"Expected unspent internal output {}-{} to be in available UTXOs.",
								output.id.tx_id,
								output.id.vout
							);
						}
					}
				}
				log::info!("Number of unspent internal outputs: {}", unspent_internal_outputs);
				log::info!(
					"Number of internal inputs remaining (not matched to any internal output): {}, value {} sats",
					all_internal_inputs.len(),
					all_internal_inputs.iter().map(|input| input.amount).sum::<u64>(),
				);
				for input in &all_internal_inputs {
					log::info!(
						"Internal input from tx {} vout {} amount {} sats",
						input.id.tx_id,
						input.id.vout,
						input.amount
					);
				}

				let num_pending_outputs = all_external_outputs.len();
				let total_external_value =
					all_external_outputs.iter().map(|(_vout, output)| output.amount).sum::<u64>();
				for (_vout, BitcoinOutput { amount, script_pubkey }) in &all_external_outputs {
					log::info!(
						"Pending output Amount: {}, Destination: {}",
						amount,
						script_pubkey.to_address(&cf_chains::btc::BitcoinNetwork::Mainnet)
					);
				}
				let total_change_outputs = all_internal_outputs.len();

				log::info!(
					"Total outputs: {} in {} transactions",
					num_pending_outputs,
					pending_tx_ids.len()
				);

				let external_pending_input_value =
					all_external_inputs.iter().map(|input| input.amount).sum::<u64>();
				let total_change_value =
					all_internal_outputs.iter().map(|output| output.amount).sum::<u64>();
				log::info!("Total external output value: {} satoshis", total_external_value);
				log::info!("Total change value: {} satoshis", total_change_value);
				log::info!("Total change outputs: {}", total_change_outputs);
				log::info!(
					"Total external pending input value: {} satoshis",
					external_pending_input_value
				);
				log::info!(
					"Total internal pending input value: {} satoshis",
					all_internal_inputs.iter().map(|input| input.amount).sum::<u64>()
				);

				log::info!("Total duplicate input value: {} satoshis", duplicate_value);
				log::info!(
					"Change address: {}",
					bitcoin_change_script.to_address(&cf_chains::btc::BitcoinNetwork::Mainnet)
				);

				for utxo in spent_utxos {
					if !all_external_inputs.remove(&utxo) {
						log::warn!(
							"Expected spent UTXO {}-{} to be in external inputs.",
							utxo.id.tx_id,
							utxo.id.vout
						);
					} else {
						log::info!(
							"Spent UTXO {}-{} found in external inputs and removed.",
							utxo.id.tx_id,
							utxo.id.vout
						);
					}
				}
				for input in &all_external_inputs {
					log::info!(
						"External input from tx {} vout {} amount {} sats",
						input.id.tx_id,
						input.id.vout,
						input.amount
					);
				}

				// --- Storage writes start here ---
				// 1. clean up broadcast storage
				// 2. remove the invalid utxo from the available utxos
				// 3. add all *external and internal* inputs back to the available utxos
				// 4. add the original *unspent* vault change utxo back to the available utxos
				// 5. batch all the pending *external* outputs
				let _ = pallet_cf_broadcast::DelayedBroadcastRetryQueue::<Runtime, BitcoinInstance>::clear(
					u32::MAX,
					None,
				);
				let _ = pallet_cf_broadcast::FailedBroadcasters::<Runtime, BitcoinInstance>::clear(
					u32::MAX,
					None,
				);
				pallet_cf_broadcast::Timeouts::<Runtime, BitcoinInstance>::kill();
				for broadcast_id in broadcast_ids {
					pallet_cf_broadcast::Pallet::<Runtime, BitcoinInstance>::clean_up_broadcast_storage(broadcast_id);
					pallet_cf_broadcast::PendingBroadcasts::<Runtime, BitcoinInstance>::mutate(
						|ids| ids.remove(&broadcast_id),
					);
					let _ = pallet_cf_broadcast::RequestSuccessCallbacks::<Runtime, BitcoinInstance>::take(broadcast_id);
				}

				pallet_cf_environment::BitcoinAvailableUtxos::<Runtime>::mutate(|utxos| {
					let initial_len = utxos.len();
					utxos.retain(|utxo| !all_internal_outputs.contains(utxo));
					let removed = initial_len.saturating_sub(utxos.len());
					log::info!("Removed {} invalid UTXOs from available UTXOs.", removed);
					utxos.extend(all_external_inputs);
					utxos.extend(all_internal_inputs);
					log::info!(
						"Total available UTXOs after re-adding external inputs: {} with {} sats",
						utxos.len(),
						utxos.iter().map(|utxo| utxo.amount).sum::<u64>(),
					);
					assert!(
						!unspendable_utxo_ids
							.iter()
							.any(|utxo_id| { utxos.iter().any(|utxo| &utxo.id == utxo_id) }),
						"Unspendable UTXO found in available UTXOs after migration."
					);
				});

				let process_txs = |utxos: &[BitcoinOutput]| {
					for utxo in utxos {
						pallet_cf_ingress_egress::Pallet::<Runtime, BitcoinInstance>::schedule_egress_no_fees(
							assets::btc::Asset::Btc,
							utxo.amount,
							utxo.script_pubkey.clone(),
						);
					}
					// let Ok(mut res) = <BitcoinApi<BtcEnvironment> as AllBatch<_>>::new_unsigned(
					// 	Default::default(),
					// 	utxos
					// 		.iter()
					// 		.map(|utxo| {
					// 			(
					// 				TransferAssetParams::<Bitcoin> {
					// 					asset: assets::btc::Asset::Btc,
					// 					amount: utxo.amount,
					// 					to: utxo.script_pubkey.clone(),
					// 				},
					// 				// Dummy egress ID
					// 				(ForeignChain::Bitcoin, 0),
					// 			)
					// 		})
					// 		.collect(),
					// ) else {
					// 	log::error!("Failed to batch pending outputs.");
					// 	return;
					// };
					// let api_call = res.pop().expect("At least one batch expected").0;
					// match api_call {
					// 	BitcoinApi::BatchTransfer(ref transfer) => {
					// 		let total_input = transfer
					// 			.bitcoin_transaction
					// 			.inputs
					// 			.iter()
					// 			.map(|input| input.amount)
					// 			.sum::<u64>();
					// 		let total_output = transfer
					// 			.bitcoin_transaction
					// 			.outputs
					// 			.iter()
					// 			.map(|output| output.amount)
					// 			.sum::<u64>();
					// 		log::info!("Created batch transfer with net value {}: total input {} sats
					// and total output {} sats", total_output as i64 - total_input as i64,
					// total_input, total_output); 	},
					// 	_ => {
					// 		unreachable!()
					// 	},
					// }
					// pallet_cf_broadcast::Pallet::<Runtime,
					// BitcoinInstance>::threshold_sign_and_broadcast(api_call, None, |_| None);
				};

				process_txs(
					&all_external_outputs
						.into_iter()
						.map(|(_vout, output)| output)
						.collect::<Vec<_>>()[..],
				);

				// Workaround to ensure the events don't get deleted: not needed using egress API
				// pallet_cf_cfe_interface::RuntimeUpgradeEvents::<Runtime>::put(
				// 	pallet_cf_cfe_interface::CfeEvents::<Runtime>::get(),
				// );

				// Alternative chunked implementation
				// However note that is doesn't work well because we need to wait for the first
				// transaction to be signed before the nxet one can be processed.

				// let mut chunks_iter = all_external_outputs
				// 	.into_iter()
				// 	.map(|(_vout, output)| output)
				// 	.array_chunks::<10>();
				// while let Some(outputs) = chunks_iter.next() {
				// 	log::info!("Processing batch of 10 pending outputs.");
				// 	process_txs(&outputs[..]);
				// }
				// if let Some(final_outputs) = chunks_iter.into_remainder() {
				// 	log::info!(
				// 		"Processing remaining batch of {} pending outputs.",
				// 		final_outputs.len()
				// 	);
				// 	process_txs(&final_outputs.collect::<Vec<_>>()[..]);
				// }
			},
			genesis_hashes::PERSEVERANCE => {
				log::info!("🧹 Clearing Solana EgressWitnessing and NonceWitnessing elections for Perseverance.");
			},
			genesis_hashes::SISYPHOS => {
				log::info!("🧹 Clearing Solana EgressWitnessing and NonceWitnessing elections for Sisyphos.");
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
