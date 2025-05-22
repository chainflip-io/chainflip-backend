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

use crate::{EthereumBroadcaster, Runtime};
use cf_runtime_utilities::genesis_hashes;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};

pub struct FetchAndEgressFromChannel;
use crate::*;
use cf_chains::{
	assets::eth::Asset,
	evm::{self, EvmFetchId, H256},
	RejectCall,
};
use core::str::FromStr;

#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

pub mod reap_old_accounts;
pub mod solana_remove_unused_channels_state;
use pallet_cf_cfe_interface::{CfeEvents, RuntimeUpgradeEvents};

pub type Migration = (
	NetworkSpecificHousekeeping,
	reap_old_accounts::Migration,
	// Can be removed once Solana address re-use is activated.
	solana_remove_unused_channels_state::SolanaRemoveUnusedChannelsState,
);

const REFUND_ADDRESS: &str = "0x39296AF2e64065D99Aaf57E03FF4d80eB6368A5b";
const REFUND_AMOUNT: u128 = 308498000000;
const CHANNEL_ADDRESS: &str = "0x52aa289646e5068185a30f79e95175f10ce6c48f";

pub struct NetworkSpecificHousekeeping;

impl OnRuntimeUpgrade for NetworkSpecificHousekeeping {
	fn on_runtime_upgrade() -> Weight {
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN =>
				if crate::VERSION.spec_version == 1_09_05 {
					CfeEvents::<Runtime>::kill();
					match <EthereumApi<EvmEnvironment> as RejectCall<Ethereum>>::new_unsigned(
						evm::DepositDetails {
							tx_hashes: Some(vec![H256::from_str(
								"0x71ddf79e091b2af49bdd023a014d33038d71cea051342aea0dd714a7efccc375",
							)
							.unwrap()]),
						},
						EthereumAddress::from_str(REFUND_ADDRESS).unwrap(),
						REFUND_AMOUNT,
						Asset::Usdt,
						Some(EvmFetchId::Fetch(
							EthereumAddress::from_str(CHANNEL_ADDRESS).unwrap(),
						)),
					) {
						Ok(reject_transaction) => {
							let broadcast_id = EthereumBroadcaster::threshold_sign_and_broadcast(
								reject_transaction,
								None,
								|_| None,
							);
							log::info!(
								"Rejected transaction successfully broadcasted with id: {:?}",
								broadcast_id
							);

							// Without doing this the events are cleared on_initialise and so the
							// engine will never see them.
							RuntimeUpgradeEvents::<Runtime>::put(CfeEvents::<Runtime>::take());
						},
						Err(e) => {
							log::error!("Failed to reject transaction: {:?}", e);
						},
					}
				} else {
					log::info!("Runtime version is not 1.9.5, skipping migration.");
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
