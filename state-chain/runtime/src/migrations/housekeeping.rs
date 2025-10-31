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
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

use crate::*;
use cf_chains::{
	assets::eth::Asset,
	evm::{self, EvmFetchId, H256},
	RejectCall,
};
use core::str::FromStr;
use pallet_cf_cfe_interface::{CfeEvents, RuntimeUpgradeEvents};

pub mod reap_old_accounts;
pub mod remove_unused_wallets_from_storage;
pub mod solana_remove_unused_channels_state;

pub type Migration = (
	NetworkSpecificHousekeeping,
	reap_old_accounts::Migration,
	// Can be removed once Solana address re-use is activated.
	solana_remove_unused_channels_state::SolanaRemoveUnusedChannelsState,
	remove_unused_wallets_from_storage::RemoveUnusedWallets,
);

pub struct NetworkSpecificHousekeeping;

impl OnRuntimeUpgrade for NetworkSpecificHousekeeping {
	fn on_runtime_upgrade() -> Weight {
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN =>
				if crate::VERSION.spec_version == 1_12_01 {
					const REFUNDS: [(&str, &str, u128, &str); 1] = [(
						"0x374e18980ef5c633fd2d0c8762f35c59ef900590",
						"0xCb22D1F41C5bd7B763aF099FFF60b2bb5A318Ce8",
						40_000_000_000,
						"0x00a312fedb2b2233f0d278052a855a491cd424bfbc11a9ac7f7d679b407d2535",
					)];

					fetch_and_egress(REFUNDS);
				} else {
					log::info!("Runtime version is not 1.12.1, skipping migration.");
				},
			genesis_hashes::PERSEVERANCE =>
				if crate::VERSION.spec_version == 1_12_00 {
					const REFUNDS: [(&str, &str, u128, &str); 1] = [(
						"0xb1c58de717b8bb809D3C5069938AD3b4cbFa6905",
						"0x83873d76B7ABf4D6da9186DAf7fcFd039d2A80c0",
						838_133_375,
						"0xa1b94635e116992295553a182a2981ef0ee3da389220d39c64249104010a222c",
					)];

					fetch_and_egress(REFUNDS);
				} else {
					log::info!("Runtime version is not 1.12.0, skipping migration.");
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

fn fetch_and_egress(refund: [(&str, &str, u128, &str); 1]) {
	CfeEvents::<Runtime>::kill();

	for (refund_address, channel_address, refund_amount, tx_hash) in refund {
		match <EthereumApi<EvmEnvironment> as RejectCall<Ethereum>>::new_unsigned(
			evm::DepositDetails { tx_hashes: Some(vec![H256::from_str(tx_hash).unwrap()]) },
			EthereumAddress::from_str(refund_address).unwrap(),
			Some(refund_amount),
			Asset::Usdt,
			Some(EvmFetchId::Fetch(EthereumAddress::from_str(channel_address).unwrap())),
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
			},
			Err(e) => {
				log::error!("Failed to reject transaction: {:?}", e);
			},
		}
	}
	// Without doing this the events are cleared on_initialise and so
	// the engine will never see them.
	RuntimeUpgradeEvents::<Runtime>::put(CfeEvents::<Runtime>::take());
}
