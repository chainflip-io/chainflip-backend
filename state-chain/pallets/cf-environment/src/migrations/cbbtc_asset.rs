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

use crate::*;

use cf_chains::evm::{Address as EvmAddress, H256};
use frame_support::{pallet_prelude::Weight, traits::UncheckedOnRuntimeUpgrade};

/// Registers the cbBTC token address as a supported Ethereum asset. Genesis only runs on new
/// chains, so existing chains need this migration to pick up the new asset.
pub struct CbbtcAssetMigration<T>(PhantomData<T>);

impl<T: Config<Hash = H256>> UncheckedOnRuntimeUpgrade for CbbtcAssetMigration<T> {
	fn on_runtime_upgrade() -> Weight {
		log::info!("🔧 Running Environment pallet cbBTC migration...");

		let cbbtc_address = match cf_runtime_utilities::genesis_hashes::genesis_hash::<T>() {
			// Mainnet cbBTC token.
			cf_runtime_utilities::genesis_hashes::BERGHAIN =>
				EvmAddress::from(hex_literal::hex!("cbB7C0000aB88B473b1f5aFd9ef808440eed33Bf")),
			// Testnets use the Sepolia cbBTC token.
			cf_runtime_utilities::genesis_hashes::PERSEVERANCE =>
				EvmAddress::from(hex_literal::hex!("cbB7C0006F23900c38EB856149F799620fcb8A4a")),
			cf_runtime_utilities::genesis_hashes::SISYPHOS =>
				EvmAddress::from(hex_literal::hex!("cbB7C0006F23900c38EB856149F799620fcb8A4a")),
			// localnet: matches the address set in the testnet chain spec.
			_ => EvmAddress::from(hex_literal::hex!("E6E340D132b5f46d1e472DebcD681B2aBc16e57E")),
		};

		EthereumSupportedAssets::<T>::insert(EthAsset::Cbbtc, cbbtc_address);

		log::info!("🔧 Environment pallet cbBTC migration completed.");

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		let expected = match cf_runtime_utilities::genesis_hashes::genesis_hash::<T>() {
			cf_runtime_utilities::genesis_hashes::BERGHAIN =>
				EvmAddress::from(hex_literal::hex!("cbB7C0000aB88B473b1f5aFd9ef808440eed33Bf")),
			cf_runtime_utilities::genesis_hashes::PERSEVERANCE =>
				EvmAddress::from(hex_literal::hex!("cbB7C0006F23900c38EB856149F799620fcb8A4a")),
			cf_runtime_utilities::genesis_hashes::SISYPHOS =>
				EvmAddress::from(hex_literal::hex!("cbB7C0006F23900c38EB856149F799620fcb8A4a")),
			_ => EvmAddress::from(hex_literal::hex!("E6E340D132b5f46d1e472DebcD681B2aBc16e57E")),
		};
		frame_support::ensure!(
			EthereumSupportedAssets::<T>::get(EthAsset::Cbbtc) == Some(expected),
			"cbBTC asset address was not set correctly after migration"
		);
		Ok(())
	}
}
