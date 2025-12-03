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

use crate::{
	chainflip::SolEnvironment, BitcoinBroadcaster, BitcoinChainTracking, BitcoinThresholdSigner,
	Runtime, RuntimeOrigin, SolanaBroadcaster,
};
use cf_chains::{
	btc::{
		api::{batch_transfer::BatchTransfer, BitcoinApi},
		deposit_address::DepositAddress,
		BitcoinOutput, ScriptPubkey, Utxo, UtxoId, BITCOIN_DUST_LIMIT,
	},
	instances::SolanaInstance,
	sol::{api::SolanaApi, SolAsset, SolanaCrypto, SolanaDepositFetchId},
	AllBatch, ChainCrypto, FetchAssetParams, SetAggKeyWithAggKey, Solana,
};
use cf_primitives::{chains::assets::btc::Asset as BtcAsset, BroadcastId};
use cf_runtime_utilities::genesis_hashes;
use cf_traits::{Chainflip, KeyProvider};
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use pallet_cf_broadcast::{BroadcastIdCounter, IncomingKeyAndBroadcastId};
use pallet_cf_environment::{SolanaAvailableNonceAccounts, SolanaUnavailableNonceAccounts};
use sp_core::H256;
use sp_runtime::AccountId32;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
use sp_std::{vec, vec::Vec};

pub mod reap_old_accounts;
pub mod solana_remove_unused_channels_state;

pub type Migration = (
	NetworkSpecificHousekeeping,
	reap_old_accounts::Migration,
	// Can be removed once Solana address re-use is activated.
	solana_remove_unused_channels_state::SolanaRemoveUnusedChannelsState,
);

const CHANNEL_ID: u32 = 95_865;
const SATS_AMOUNT: u64 = 1_0105_0000;
const DESTINATION: &str = "bc1qeclrpp65qx5q7qup4eey3z77xkfd5tej7q8mvg";

fn utxos() -> [Utxo; 1] {
	[Utxo {
		id: UtxoId {
			// 1deeb655829c3a42491c280e1a143870915649406a2ac3ab166144628d709bcc reversed byte
			// order
			tx_id: H256(hex_literal::hex!(
				"cc9b708d62446116abc32a6a404956917038141a0e281c49423a9c8255b6ee1d"
			)),
			vout: 0,
		},
		amount: SATS_AMOUNT,
		// This evaluates to the deposit address
		// bc1plh6vv3u33y2w0cxz6gza2w99xqnlx4uy7gtmjf82ksxklezktu9s4045rj (see unit test below)
		deposit_address: DepositAddress::new(
			// Vault pubkey.
			hex_literal::hex!("233104575f36e0bf0f74a529cf465feff636371e4f65dfa57a517350e213de8a"),
			// Channel ID.
			CHANNEL_ID,
		),
	}]
}

#[test]
fn f() {
	let utxos = utxos();
	println!(
		"{}",
		utxos[0]
			.deposit_address
			.script_pubkey()
			.to_address(&cf_chains::btc::BitcoinNetwork::Mainnet),
	);
	// Check that the destination address is valid.
	ScriptPubkey::try_from_address(DESTINATION, &cf_chains::btc::BitcoinNetwork::Mainnet).unwrap();
}

fn recover_all_solana_nonces() {
	SolanaAvailableNonceAccounts::<Runtime>::mutate(|available| {
		available.extend(SolanaUnavailableNonceAccounts::<Runtime>::drain())
	});
}

fn delete_broadcast_from_broadcaster_pallet(id: BroadcastId) {
	SolanaBroadcaster::remove_pending_broadcast(&id);
	SolanaBroadcaster::clean_up_broadcast_storage(id);
}

fn resubmit_solana_rotation(
	new_agg_key: <SolanaCrypto as ChainCrypto>::AggKey,
	epoch_index: cf_primitives::EpochIndex,
	participants: sp_std::collections::btree_set::BTreeSet<<Runtime as Chainflip>::ValidatorId>,
	signers_required: u32,
	max_retries: u32,
) {
	type CurrentKeyEpoch = pallet_cf_threshold_signature::CurrentKeyEpoch<Runtime, SolanaInstance>;

	// we have to build the call with the previous epochs key, so we temporarily rewind the epoch
	let actual_epoch_index = CurrentKeyEpoch::get();
	CurrentKeyEpoch::set(Some(epoch_index));

	let rotation_call =
		<SolanaApi<SolEnvironment> as SetAggKeyWithAggKey<SolanaCrypto>>::new_unsigned_impl(
			None,        // the `old_key` argument is ignored by solana
			new_agg_key, // the new agg key we want to rotate to
		);

	// reset the epoch index to the actual value after building the call
	CurrentKeyEpoch::set(actual_epoch_index);

	match rotation_call {
		Ok(Some(api_call)) => {
			// we predict the broadcast id
			let broadcast_id = BroadcastIdCounter::<Runtime, SolanaInstance>::get() + 1;

			let origin = RuntimeOrigin::from(pallet_cf_governance::RawOrigin::GovernanceApproval);
			let broadcast = true;
			match SolanaBroadcaster::threshold_sign_and_broadcast_with_historical_key(
				origin,
				scale_info::prelude::boxed::Box::new(api_call),
				epoch_index,
				participants,
				signers_required,
				max_retries,
				broadcast,
			) {
				Ok(_) => log::info!("successfully executed runtime call"),
				Err(err) => log::error!("encountered error: {err:?}"),
			}

			// update the incoming key and broadcast request
			IncomingKeyAndBroadcastId::<Runtime, SolanaInstance>::put((new_agg_key, broadcast_id));
		},
		Ok(None) => {
			log::error!("Could not build rotation api call: returned None");
		},
		Err(err) => {
			log::error!("Could not build rotation api call: {err:?}");
		},
	}
}

fn resubmit_solana_fetch(fetches: Vec<FetchAssetParams<Solana>>) {
	let fetch_calls =
		<SolanaApi<SolEnvironment> as AllBatch<Solana>>::new_unsigned_impl(fetches, vec![]);

	match fetch_calls {
		Ok(fetch_calls) =>
			for (fetch_call, egress_id) in fetch_calls {
				let (broadcast_id, ts_id) =
					SolanaBroadcaster::threshold_sign_and_broadcast(fetch_call, None, |_| None);
				log::info!(
					"signed fetch call for egress ids {egress_id:?}: {broadcast_id}:{ts_id}"
				);
			},
		Err(err) => {
			log::error!("Could not build fetch api call: {err:?}");
		},
	}
}

pub struct NetworkSpecificHousekeeping;

impl OnRuntimeUpgrade for NetworkSpecificHousekeeping {
	fn on_runtime_upgrade() -> Weight {
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => {
				let utxos = utxos();
				if crate::VERSION.spec_version == 1_12_03 {
					use cf_chains::FeeEstimationApi;
					use pallet_cf_cfe_interface::{CfeEvents, RuntimeUpgradeEvents};

					const BTC: BtcAsset = BtcAsset::Btc;

					// Clear out any old events.
					CfeEvents::<Runtime>::kill();
					RuntimeUpgradeEvents::<Runtime>::kill();

					log::info!("🧹 Doing housekeeping.");
					let fee_estimator = BitcoinChainTracking::chain_state()
						.expect("Chain state always exists")
						.tracked_data;
					let ingress_fee = fee_estimator
						.estimate_fee(BTC, cf_primitives::IngressOrEgress::IngressVaultSwap);
					let egress_fee =
						fee_estimator.estimate_fee(BTC, cf_primitives::IngressOrEgress::Egress);
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
								[BitcoinOutput {
									amount: SATS_AMOUNT.saturating_sub(fee_estimate),
									script_pubkey: ScriptPubkey::try_from_address(
										DESTINATION,
										&cf_chains::btc::BitcoinNetwork::Mainnet,
									)
									.expect("Valid address, checked via unit test"),
								}]
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
				if crate::VERSION.spec_version == 1_12_06 {
					log::info!("🧹 Resubmitting solana rotation tx for Perseverance.");

					// Clear out any old events.
					use pallet_cf_cfe_interface::{CfeEvents, RuntimeUpgradeEvents};
					CfeEvents::<Runtime>::kill();
					RuntimeUpgradeEvents::<Runtime>::kill();

					// recover all nonces
					recover_all_solana_nonces();

					// delete the old rotation + all other stuck pending api calls
					let broadcast_ids =
						[2432, 2433, 2434, 2435, 2436, 2437, 2438, 2439, 2440, 2441, 2442];
					for id in broadcast_ids {
						delete_broadcast_from_broadcaster_pallet(id);
					}

					resubmit_solana_rotation(
						// new agg key, double check!
						cf_chains::sol::SolAddress(hex_literal::hex!(
							"f24ab9a36f9156b1e3f8920d75ebeb897e53951fc1c847056375540a102f16b9"
						)),
						769, // the current epoch is 770, we want to sign for the previous one
						[
							AccountId32::new(hex_literal::hex![
								"54550938f444501c7bfe162806226cb2329a573077bb8bb8d52a770e2c6eae12"
							]),
							AccountId32::new(hex_literal::hex![
								"789523326e5f007f7643f14fa9e6bcfaaff9dd217e7e7a384648a46398245d55"
							]),
							// external validator:
							// AccountId32::new(hex_literal::hex![
							// 	"62c3f505c6c9ff480c83942c4946153c08f02dc5e93b9431590b872296810878"
							// ]),
							AccountId32::new(hex_literal::hex![
								"7a4738071f16c71ef3e5d94504d472fdf73228cb6a36e744e0caaf13555c3c01"
							]),
							// external validator:
							// AccountId32::new(hex_literal::hex![
							// 	"169805dd9c7b0c1c4881fd8a3f98483b27c1c04dcea44b1f1bd502926be2a37b"
							// ]),
							AccountId32::new(hex_literal::hex![
								"3e666e445cb15b5469cac9cbb2e3aec6e2f88ff28435353c35e7172aaa9b7c18"
							]),
							AccountId32::new(hex_literal::hex![
								"7a467c9e1722b35408618a0cffc87c1e8433798e9c5a79339a10d71ede9e9d79"
							]),
							AccountId32::new(hex_literal::hex![
								"fea6a1ae1029da56f4df3eb886b81443c97247f350e0b7faeb805ff747d84f70"
							]),
							// external validator:
							// AccountId32::new(hex_literal::hex![
							// 	"38bfc9cb271c312e5dfc8a675e42f61f3297fce72702d9d1a3e35dc5813d9c04"
							// ]),
							AccountId32::new(hex_literal::hex![
								"e4905f40c45d7951d25587defefda85e0f148bdeb3fd04cb5fd8bef5af9abc21"
							]),
						]
						.into_iter()
						.collect(),
						7, // required signers
						2, // num of retries
					);

					resubmit_solana_fetch(vec![FetchAssetParams {
						deposit_fetch_id: SolanaDepositFetchId {
							channel_id: 2333,
							address: sol_prim::consts::const_address(
								"HV99NDRovJpGyty6DfHwh7Rj37abLCceXeFysKkxhnvD",
							),
							bump: 251,
						},
						asset: SolAsset::Sol,
					}]);

					// Move events into the runtime upgrade events.
					RuntimeUpgradeEvents::<Runtime>::put(CfeEvents::<Runtime>::take());
				}
			},
			genesis_hashes::SISYPHOS => {
				if crate::VERSION.spec_version == 1_12_06 {
					log::info!("🧹 Resubmitting solana rotation tx for Sisyphos.");

					// Clear out any old events.
					use pallet_cf_cfe_interface::{CfeEvents, RuntimeUpgradeEvents};
					CfeEvents::<Runtime>::kill();
					RuntimeUpgradeEvents::<Runtime>::kill();

					// recover all nonces
					recover_all_solana_nonces();

					// delete the old broadcast
					delete_broadcast_from_broadcaster_pallet(2933);

					// create the new one
					resubmit_solana_rotation(
						// new agg key, double check!
						cf_chains::sol::SolAddress(hex_literal::hex!(
							"beca85b4bcecd87cb6f7d76e1d2652240400e1d30d8b13468fd23c0dffa40487"
						)),
						4733, // current epoch is 4734
						[
							AccountId32::new(hex_literal::hex![
								"7a47312f9bd71d480b1e8f927fe8958af5f6345ac55cb89ef87cff5befcb0949"
							]),
							AccountId32::new(hex_literal::hex![
								"7a46817c60dff154901510e028f865300452a8d7a528f573398313287c689929"
							]),
						]
						.into_iter()
						.collect(),
						2, // required signers
						2, // num retries
					);

					// Move events into the runtime upgrade events.
					RuntimeUpgradeEvents::<Runtime>::put(CfeEvents::<Runtime>::take());
				}
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
			genesis_hashes::PERSEVERANCE => {
				let id_counter =
					pallet_cf_broadcast::BroadcastIdCounter::<Runtime, SolanaInstance>::get();
				Ok(id_counter.to_le_bytes().to_vec())
			},
			genesis_hashes::SISYPHOS => {
				let id_counter =
					pallet_cf_broadcast::BroadcastIdCounter::<Runtime, SolanaInstance>::get();
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
				if crate::VERSION.spec_version == 1_12_03 {
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
			genesis_hashes::PERSEVERANCE =>
				if crate::VERSION.spec_version == 1_12_06 {
					let old_id_counter = u32::from_le_bytes(
						state
							.try_into()
							.map_err(|_| DispatchError::Other("Invalid pre-upgrade state"))?,
					);
					let new_id_counter =
						pallet_cf_broadcast::BroadcastIdCounter::<Runtime, SolanaInstance>::get();
					assert!(
						new_id_counter - old_id_counter == 2,
						"Expected exactly two new broadcast",
					);
					assert!(
						pallet_cf_broadcast::PendingBroadcasts::<Runtime, SolanaInstance>::get()
							.contains(&new_id_counter) &&
							pallet_cf_broadcast::PendingBroadcasts::<Runtime, SolanaInstance>::get(
							)
							.contains(&(new_id_counter - 1)),
						"New broadcasts should be pending"
					);
				},
			genesis_hashes::SISYPHOS =>
				if crate::VERSION.spec_version == 1_12_06 {
					let old_id_counter = u32::from_le_bytes(
						state
							.try_into()
							.map_err(|_| DispatchError::Other("Invalid pre-upgrade state"))?,
					);
					let new_id_counter =
						pallet_cf_broadcast::BroadcastIdCounter::<Runtime, SolanaInstance>::get();
					assert!(
						new_id_counter - old_id_counter == 1,
						"Expected exactly one new broadcast",
					);
					assert!(
						pallet_cf_broadcast::PendingBroadcasts::<Runtime, SolanaInstance>::get()
							.contains(&new_id_counter),
						"New broadcast should be pending"
					);
				},
			_ => {},
		}

		Ok(())
	}
}
