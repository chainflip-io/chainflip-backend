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

use crate::{BitcoinChainTracking, BitcoinIngressEgress, Runtime};
use cf_chains::{
	btc::{deposit_address::DepositAddress, ScriptPubkey, Utxo, UtxoId},
	Bitcoin, DepositChannel,
};
use cf_primitives::chains::assets::btc::Asset as BtcAsset;
use cf_runtime_utilities::genesis_hashes;
use cf_traits::GetBlockHeight;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use pallet_cf_ingress_egress::{
	BoostStatus, ChannelAction, DepositChannelDetails, DepositChannelLookup,
	DepositChannelRecycleBlocks, DepositWitness,
};
use sp_core::H256;
use sp_runtime::AccountId32;
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

const UTXO_ID: UtxoId = UtxoId {
	tx_id: H256(hex_literal::hex!(
		"68dd89d55d9ed813cdd1629385139db04b1be5e09cdde63f8204b31a9838c7a0"
	)),
	vout: 5,
};
const CHANNEL_ID: u32 = 104_657;
const DEPOSIT_AMOUNT: u64 = 2_9999_9759; // 0.299999759 BTC

fn utxo() -> Utxo {
	Utxo {
		id: UTXO_ID,
		amount: DEPOSIT_AMOUNT,
		deposit_address: DepositAddress::new(
			// Vault pubkey.
			hex_literal::hex!("019e11f08f9278f40f9e942216df66141b5bb1a293e6b7ea607b92ec14c0df72"),
			// Channel ID.
			CHANNEL_ID,
		),
	}
}

pub struct NetworkSpecificHousekeeping;

impl OnRuntimeUpgrade for NetworkSpecificHousekeeping {
	fn on_runtime_upgrade() -> Weight {
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => {
				if crate::VERSION.spec_version == 1_10_04 &&
					!pallet_cf_environment::BitcoinAvailableUtxos::<Runtime>::get()
						.iter()
						.any(|utxo| utxo.id == UTXO_ID)
				{
					log::info!("🧹 Adding missing UTXO.");

					const DEPOSIT_HEIGHT: u64 = 911335;

					let utxo = utxo();
					let address = utxo.deposit_address.script_pubkey();
					let owner = AccountId32::new(hex_literal::hex!(
						"745e494dd1f535898b3f5429998ac76128fafc326e6fcfdf2ead98159849e63e"
					));

					DepositChannelRecycleBlocks::<Runtime, crate::BitcoinInstance>::append((
						BitcoinChainTracking::get_block_height() + 10,
						address.clone(),
					));
					DepositChannelLookup::<Runtime, crate::BitcoinInstance>::insert(
						&address,
						DepositChannelDetails {
							owner: owner.clone(),
							deposit_channel: DepositChannel {
								channel_id: CHANNEL_ID as u64,
								address: address.clone(),
								asset: BtcAsset::Btc,
								state: utxo.deposit_address.clone(),
							},
							opened_at: DEPOSIT_HEIGHT - 5,
							expires_at: DEPOSIT_HEIGHT + 5,
							action: ChannelAction::LiquidityProvision {
								lp_account: owner.clone(),
								refund_address: cf_chains::ForeignChainAddress::Btc(
									ScriptPubkey::P2WPKH(hex_literal::hex!(
										"71b56ddbc1f23f4f199e71b0631ffb8a1adac714"
									)),
								),
							},
							boost_fee: 0,
							boost_status: BoostStatus::NotBoosted,
						},
					);

					BitcoinIngressEgress::process_channel_deposit_full_witness(
						DepositWitness::<Bitcoin> {
							deposit_address: utxo.deposit_address.script_pubkey(),
							asset: BtcAsset::Btc,
							amount: utxo.amount,
							deposit_details: utxo,
						},
						// Block height
						DEPOSIT_HEIGHT,
					);
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
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		use codec::Encode;
		use sp_core::crypto::Ss58Codec;
		use sp_runtime::AccountId32;
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => {
				if crate::VERSION.spec_version == 1_10_04 &&
					!pallet_cf_environment::BitcoinAvailableUtxos::<Runtime>::get()
						.iter()
						.any(|utxo| utxo.id == UTXO_ID)
				{
					assert_eq!(
						utxo().deposit_address.script_pubkey(),
						// Lookup-Table key
						ScriptPubkey::Taproot(hex_literal::hex!(
							"815d0dd0cf3fe7f8adb1d4fa56c089948d59cac19c13096d801d20df38c0dc3d"
						)),
					);

					let utxos = pallet_cf_environment::BitcoinAvailableUtxos::<Runtime>::get();
					let balance = pallet_cf_asset_balances::FreeBalances::<Runtime>::get(
						AccountId32::from_ss58check(
							"cFLW4PhasdivcJKuA2BGw9Y9dz7EFwks82K8Z6U3MfCk8WcNW",
						)
						.expect("Address should be correct"),
						cf_primitives::Asset::Btc,
					);
					Ok((utxos, balance).encode())
				} else {
					Ok(Default::default())
				}
			},
			_ => Ok(Default::default()),
		}
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		use codec::Decode;
		use sp_core::crypto::Ss58Codec;
		use sp_runtime::AccountId32;
		use sp_std::{collections::btree_set::BTreeSet, vec};

		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN =>
				if state.len() > 0 {
					use pallet_cf_ingress_egress::AmountAndFeesWithheld;

					let (pre_utxos, pre_balance) =
						<(BTreeSet<Utxo>, u128)>::decode(&mut &state[..])
							.map_err(|_| "Failed to decode pre-upgrade state")?;
					let post_utxos = BTreeSet::from_iter(
						pallet_cf_environment::BitcoinAvailableUtxos::<Runtime>::get(),
					);
					let post_balance = pallet_cf_asset_balances::FreeBalances::<Runtime>::get(
						AccountId32::from_ss58check(
							"cFLW4PhasdivcJKuA2BGw9Y9dz7EFwks82K8Z6U3MfCk8WcNW",
						)
						.expect("Address should be correct"),
						cf_primitives::Asset::Btc,
					);

					let utxo = utxo();
					assert_eq!(
						post_utxos.difference(&pre_utxos).collect::<Vec<_>>(),
						vec![&utxo],
						"Pre: {pre_utxos:?}, Post: {post_utxos:?}"
					);
					let AmountAndFeesWithheld { amount_after_fees, .. } = pallet_cf_ingress_egress::Pallet::<
						Runtime,
						crate::BitcoinInstance,
					>::withhold_ingress_or_egress_fee(
						pallet_cf_ingress_egress::IngressOrEgress::IngressDepositChannel,
						BtcAsset::Btc,
						DEPOSIT_AMOUNT,
					);
					assert_eq!(post_balance - pre_balance, amount_after_fees as u128);
				} else {
					log::info!("🧹 No housekeeping was required.");
				},
			_ => return Ok(()),
		}

		Ok(())
	}
}
