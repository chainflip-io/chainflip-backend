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

use crate::{Instance15, Instance6, Runtime};
use cf_chains::instances::AssethubInstance;
use cf_traits::SafeMode;
use frame_support::{
	traits::{OnRuntimeUpgrade, UncheckedOnRuntimeUpgrade},
	weights::Weight,
};
use sp_core::H256;

pub mod old {
	use crate::*;
	use frame_support::pallet_prelude::*;

	#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
	pub struct RuntimeSafeMode {
		pub emissions: pallet_cf_emissions::PalletSafeMode,
		pub funding: pallet_cf_funding::PalletSafeMode,
		pub swapping: pallet_cf_swapping::PalletSafeMode,
		pub liquidity_provider: pallet_cf_lp::PalletSafeMode,
		pub validator: pallet_cf_validator::PalletSafeMode,
		pub pools: pallet_cf_pools::PalletSafeMode,
		pub reputation: pallet_cf_reputation::PalletSafeMode,
		pub asset_balances: pallet_cf_asset_balances::PalletSafeMode,
		pub threshold_signature_evm: pallet_cf_threshold_signature::PalletSafeMode<Instance16>,
		pub threshold_signature_bitcoin: pallet_cf_threshold_signature::PalletSafeMode<Instance3>,
		pub threshold_signature_polkadot: pallet_cf_threshold_signature::PalletSafeMode<Instance2>,
		pub threshold_signature_solana: pallet_cf_threshold_signature::PalletSafeMode<Instance5>,
		pub broadcast_ethereum: pallet_cf_broadcast::PalletSafeMode<Instance1>,
		pub broadcast_bitcoin: pallet_cf_broadcast::PalletSafeMode<Instance3>,
		pub broadcast_polkadot: pallet_cf_broadcast::PalletSafeMode<Instance2>,
		pub broadcast_arbitrum: pallet_cf_broadcast::PalletSafeMode<Instance4>,
		pub broadcast_solana: pallet_cf_broadcast::PalletSafeMode<Instance5>,
		pub witnesser: pallet_cf_witnesser::PalletSafeMode<WitnesserCallPermission>,
		pub ingress_egress_ethereum: pallet_cf_ingress_egress::PalletSafeMode<Instance1>,
		pub ingress_egress_bitcoin: pallet_cf_ingress_egress::PalletSafeMode<Instance3>,
		pub ingress_egress_polkadot: pallet_cf_ingress_egress::PalletSafeMode<Instance2>,
		pub ingress_egress_arbitrum: pallet_cf_ingress_egress::PalletSafeMode<Instance4>,
		pub ingress_egress_solana: pallet_cf_ingress_egress::PalletSafeMode<Instance5>,
	}
}

pub struct AssethubChainstate;

impl OnRuntimeUpgrade for AssethubChainstate {
	fn on_runtime_upgrade() -> Weight {
		if pallet_cf_chain_tracking::CurrentChainState::<Runtime, AssethubInstance>::get().is_none()
		{
			pallet_cf_chain_tracking::CurrentChainState::<Runtime, AssethubInstance>::put(
				cf_chains::ChainState {
					block_height: 0,
					tracked_data: cf_chains::hub::AssethubTrackedData {
						median_tip: 0,
						runtime_version: cf_chains::dot::RuntimeVersion {
							spec_version: 1004000,
							transaction_version: 15,
						},
					},
				},
			);
		}
		Weight::zero()
	}
}

pub struct AssethubUpdate;

impl UncheckedOnRuntimeUpgrade for AssethubUpdate {
	fn on_runtime_upgrade() -> Weight {
		// Initialize Assethub derived account id
		pallet_cf_environment::AssethubOutputAccountId::<Runtime>::set(1);

		// Update Assethub Genesis hash
		match cf_runtime_utilities::genesis_hashes::genesis_hash::<Runtime>() {
			cf_runtime_utilities::genesis_hashes::BERGHAIN => {
				pallet_cf_environment::AssethubGenesisHash::<Runtime>::put(H256(
					hex_literal::hex!(
						"68d56f15f85d3136970ec16946040bc1752654e906147f7e43e9d539d7c3de2f" /* Assethub mainnet */
					),
				));
			},
			cf_runtime_utilities::genesis_hashes::PERSEVERANCE => {
				pallet_cf_environment::AssethubGenesisHash::<Runtime>::put(H256(
					hex_literal::hex!(
						"4fb7a1b11ba4a38827cf211b3effc87971413e4a9fd79c6bcc2c633383496832" /* Assethub in PDot */
					),
				));
			},
			cf_runtime_utilities::genesis_hashes::SISYPHOS => {
				pallet_cf_environment::AssethubGenesisHash::<Runtime>::put(H256(
					hex_literal::hex!(
						"d6ca94b515c4693ca4acc8a04afa935572c2896a796b691848f075d5749c6afc" /* Assethub in SisyDot */
					),
				));
			},
			_ => {
				pallet_cf_environment::AssethubGenesisHash::<Runtime>::put(H256(
					hex_literal::hex!(
						"e58c46099b158aeb474d1020ea706f468d4edfa27e6e3e75688da1bb17fd6876" /* Assethub on localnet */
					),
				));
			},
		}

		// Update runtime safemode
		let _ = pallet_cf_environment::RuntimeSafeMode::<Runtime>::translate(
			|maybe_old: Option<old::RuntimeSafeMode>| {
				maybe_old.map(|old| crate::safe_mode::RuntimeSafeMode {
					emissions: old.emissions,
					funding: old.funding,
					swapping: old.swapping,
					liquidity_provider: old.liquidity_provider,
					validator: old.validator,
					pools: old.pools,
					trading_strategies:
						<pallet_cf_trading_strategy::PalletSafeMode as SafeMode>::CODE_GREEN,
					reputation: old.reputation,
					asset_balances: old.asset_balances,
					threshold_signature_evm: old.threshold_signature_evm,
					threshold_signature_bitcoin: old.threshold_signature_bitcoin,
					threshold_signature_polkadot: <pallet_cf_threshold_signature::PalletSafeMode<
						Instance15,
					> as SafeMode>::CODE_GREEN,
					threshold_signature_solana: old.threshold_signature_solana,
					broadcast_ethereum: old.broadcast_ethereum,
					broadcast_bitcoin: old.broadcast_bitcoin,
					broadcast_polkadot: old.broadcast_polkadot,
					broadcast_arbitrum: old.broadcast_arbitrum,
					broadcast_solana: old.broadcast_solana,
					broadcast_assethub:
						<pallet_cf_broadcast::PalletSafeMode<Instance6> as SafeMode>::CODE_GREEN,
					witnesser: old.witnesser,
					ingress_egress_ethereum: old.ingress_egress_ethereum,
					ingress_egress_bitcoin: old.ingress_egress_bitcoin,
					ingress_egress_polkadot: old.ingress_egress_polkadot,
					ingress_egress_arbitrum: old.ingress_egress_arbitrum,
					ingress_egress_solana: old.ingress_egress_solana,
					ingress_egress_assethub: <pallet_cf_ingress_egress::PalletSafeMode<
						Instance6,
					> as SafeMode>::CODE_GREEN,
				})
			},
		).map_err(|_| {
			log::warn!("Migration for Runtime Safe mode was not able to interpret the existing storage in the old format!")
		});

		Weight::zero()
	}
}
