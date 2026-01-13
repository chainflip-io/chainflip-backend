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

use crate::{chainflip::EvmEnvironment, Runtime};
use cf_chains::{assets, AllBatch, ForeignChain, TransferAssetParams};
use cf_runtime_utilities::genesis_hashes;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
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

pub struct NetworkSpecificHousekeeping;

impl OnRuntimeUpgrade for NetworkSpecificHousekeeping {
	fn on_runtime_upgrade() -> Weight {
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => {
				if crate::VERSION.spec_version != 2_00_07 {
					log::info!("🧹 No housekeeping required for Berghain.");
					return Weight::zero();
				}

				log::info!("🧹 Performing Ethereum broadcasts for Berghain housekeeping.");
				let Ok(mut res) = <cf_chains::eth::api::EthereumApi<EvmEnvironment> as AllBatch<
					_,
				>>::new_unsigned(
					Default::default(),
					(0..) // Dummy egress_ids: these aren't used.
						.zip([
							// https://etherscan.io/tx/0x35c71da922b12f0f9b15b279be46ee307a82748262116a3053fd7dd87dfacb9e
							TransferAssetParams {
								asset: assets::eth::Asset::Eth,
								amount: 1_000_000_000_000_000_000,
								to: hex_literal::hex!("e9b5Cf76dFCca58aFDf04Ac8b76633B4BCeADa38")
									.into(),
							},
						])
						.map(|(a, b)| (b, (ForeignChain::Ethereum, a)))
						.collect(),
				) else {
					log::error!("Failed to construct Ethereum batch for Berghain housekeeping.");
					return Weight::zero();
				};
				let Some((api_call, _)) = res.pop() else {
					log::info!("Unexpected error.");
					return Weight::zero();
				};
				crate::EthereumBroadcaster::threshold_sign_and_broadcast(api_call, None, |_| None);

				// Without doing this the events are cleared on_initialise and so
				// the engine will never see them.
				pallet_cf_cfe_interface::RuntimeUpgradeEvents::<Runtime>::put(
					pallet_cf_cfe_interface::CfeEvents::<Runtime>::take(),
				);
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
