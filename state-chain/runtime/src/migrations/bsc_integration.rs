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
use cf_chains::instances::{BitcoinInstance, BscInstance};
#[cfg(feature = "try-runtime")]
use codec::Encode;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

pub struct BscElectionsInit;

impl OnRuntimeUpgrade for BscElectionsInit {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(().encode())
	}

	fn on_runtime_upgrade() -> Weight {
		let result = pallet_cf_elections::Pallet::<Runtime, BscInstance>::internally_initialize(
			crate::chainflip::witnessing::bsc_elections::initial_state(),
		);
		if result.is_err() {
			log::info!("BSC Elections already initialised.");
		}
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		use pallet_cf_elections::{ElectoralUnsynchronisedSettings, SharedDataReferenceLifetime};

		let initial_state = crate::chainflip::witnessing::bsc_elections::initial_state();

		assert_eq!(
			ElectoralUnsynchronisedSettings::<Runtime, BscInstance>::get(),
			Some(initial_state.unsynchronised_settings)
		);
		assert_eq!(
			SharedDataReferenceLifetime::<Runtime, BscInstance>::get(),
			initial_state.shared_data_reference_lifetime
		);

		Ok(())
	}
}

/// Initialize Bsc ingress-egress pallet values (deposit channel lifetime and whitelisted brokers).
/// These are normally set via GenesisConfig but must be set via migration when adding a new chain.
/// Note: WitnessSafetyMargin is deprecated for chains using elections-based witnessing (see
/// comment on WitnessSafetyMargin storage item), so we only set the channel lifetime here.
pub struct BscIngressEgressInit;

impl OnRuntimeUpgrade for BscIngressEgressInit {
	fn on_runtime_upgrade() -> Weight {
		use cf_runtime_utilities::genesis_hashes;

		// Values from each chain_spec (node/src/chain_spec/{berghain,testnet,devnet}.rs),
		// based on BSC's 450ms block time.
		let deposit_channel_lifetime: u64 = match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => 24 * 3600 * 1000 / 450,
			genesis_hashes::PERSEVERANCE | genesis_hashes::SISYPHOS => 2 * 60 * 60 * 1000 / 450,
			_ => 10 * 60 * 1000 / 450,
		};

		log::info!(
			"🔧 Initializing Bsc ingress-egress: deposit_channel_lifetime={}",
			deposit_channel_lifetime,
		);

		pallet_cf_ingress_egress::DepositChannelLifetime::<Runtime, BscInstance>::put(
			deposit_channel_lifetime,
		);

		for id in
			pallet_cf_ingress_egress::WhitelistedBrokers::<Runtime, BitcoinInstance>::iter_keys()
		{
			pallet_cf_ingress_egress::WhitelistedBrokers::<Runtime, BscInstance>::insert(id, ());
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		let lifetime =
			pallet_cf_ingress_egress::DepositChannelLifetime::<Runtime, BscInstance>::get();
		frame_support::ensure!(lifetime > 0, "Bsc deposit channel lifetime must be non-zero");

		frame_support::ensure!(
			pallet_cf_ingress_egress::WhitelistedBrokers::<Runtime, BscInstance>::iter_keys()
				.count() == pallet_cf_ingress_egress::WhitelistedBrokers::<Runtime, BitcoinInstance>::iter_keys().count(),
			"Bsc whitelisted brokers not migrated correctly"
		);

		Ok(())
	}
}

/// Initialize BscChainTracking with initial chain state.
pub struct BscChainstate;

impl OnRuntimeUpgrade for BscChainstate {
	fn on_runtime_upgrade() -> Weight {
		if pallet_cf_chain_tracking::CurrentChainState::<Runtime, BscInstance>::get().is_none() {
			log::info!("🔧 Initializing BscChainTracking with block_height 0...");
			// Same values as the chain_spec genesis config.
			pallet_cf_chain_tracking::CurrentChainState::<Runtime, BscInstance>::put(
				cf_chains::ChainState {
					block_height: 0u64,
					tracked_data: cf_chains::bsc::BscTrackedData {
						priority_fee: 10_000_000_000u64.into(),
					},
				},
			);
		}
		Weight::zero()
	}
}

/// Seed the Bsc broadcast timeout. Without it, `BroadcastTimeout` defaults to 100 bsc blocks
pub struct BscBroadcasterInit;

impl OnRuntimeUpgrade for BscBroadcasterInit {
	fn on_runtime_upgrade() -> Weight {
		// Same value as the chain_spec genesis config.
		let broadcast_timeout = 2 * crate::constants::common::BLOCKS_PER_MINUTE_BSC;
		if !pallet_cf_broadcast::BroadcastTimeout::<Runtime, BscInstance>::exists() {
			log::info!("🔧 Initializing Bsc broadcast timeout: {} blocks...", broadcast_timeout);
			pallet_cf_broadcast::BroadcastTimeout::<Runtime, BscInstance>::put(broadcast_timeout);
		}
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		frame_support::ensure!(
			pallet_cf_broadcast::BroadcastTimeout::<Runtime, BscInstance>::get() ==
				2 * crate::constants::common::BLOCKS_PER_MINUTE_BSC,
			"Bsc broadcast timeout not initialized correctly"
		);
		Ok(())
	}
}
