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
				if crate::VERSION.spec_version != 2_00_06 {
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
							// https://etherscan.io/tx/0xa701b0c4b0b275ab2540dae384031ddb0aaa2b13c0b571da31b920b927b1d1b4
							TransferAssetParams {
								asset: assets::eth::Asset::Eth,
								amount: 91_500_000_000_000_000_000,
								to: hex_literal::hex!("C26b5977C42C4fa2DD41750F8658f6Bd2B67869C")
									.into(),
							},
							// https://etherscan.io/tx/0x883888a29b56ef8ad37f74dec7acdbe8962d670dc48ad5d86de499b446847a56
							TransferAssetParams {
								asset: assets::eth::Asset::Eth,
								amount: 34_000_000_000_000_000_000,
								to: hex_literal::hex!("2D4f72825c5908b6fcA5a624F1B412b6E1D79bb4")
									.into(),
							},
							// https://etherscan.io/tx/0x99313cf3457b663a5077534b5fbee46c1effad136dcfb51019550acc200a0184
							TransferAssetParams {
								asset: assets::eth::Asset::Flip,
								amount: 1_927_000_000_000_000_000_000,
								to: hex_literal::hex!("734Ec340250d3268E7D7104aEdaa426686345504")
									.into(),
							},
							// https://etherscan.io/tx/0x4fea3b54d41a6721928ad63421742da2201be084ca9c5e35ac1bb9b9d414897c#internal
							TransferAssetParams {
								asset: assets::eth::Asset::Eth,
								amount: 1_343_638_500_000_000_000,
								to: hex_literal::hex!("2c02eea3ad478320f6629f1b01352c690a48588a")
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
