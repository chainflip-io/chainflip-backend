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

pub struct BscAssetsMigration<T>(PhantomData<T>);

impl<T: Config<Hash = H256>> UncheckedOnRuntimeUpgrade for BscAssetsMigration<T> {
	fn on_runtime_upgrade() -> Weight {
		log::info!("🔧 Running Environment pallet Bsc migration...");

		let (
			chain_id,
			bsc_usdt_address,
			key_manager_address,
			vault_address,
			address_checker_address,
		) = match cf_runtime_utilities::genesis_hashes::genesis_hash::<T>() {
			cf_runtime_utilities::genesis_hashes::BERGHAIN => (
				cf_chains::bsc::CHAIN_ID_MAINNET,
				EvmAddress::from(hex_literal::hex!("55d398326f99059fF775485246999027B3197955")),
				EvmAddress::from(hex_literal::hex!("BFe612c77C2807Ac5a6A41F84436287578000275")),
				EvmAddress::from(hex_literal::hex!("79001a5e762f3bEFC8e5871b42F6734e00498920")),
				EvmAddress::from(hex_literal::hex!("c1B12993f760B654897F0257573202fba13D5481")),
			),
			cf_runtime_utilities::genesis_hashes::PERSEVERANCE => (
				cf_chains::bsc::CHAIN_ID_TESTNET,
				EvmAddress::from(hex_literal::hex!("337610d27c682E347C9cD60BD4b3b107C9d34dDd")),
				EvmAddress::from(hex_literal::hex!("77864880BA7D2F8A95d540D85233A250EcfafDc0")),
				EvmAddress::from(hex_literal::hex!("98b9829Cc96e910B1253163E708e4cBF3F5BE277")),
				EvmAddress::from(hex_literal::hex!("0fDA3D36ce05531F1cb14E519672dd52C314Fd28")),
			),
			cf_runtime_utilities::genesis_hashes::SISYPHOS => (
				cf_chains::bsc::CHAIN_ID_TESTNET,
				EvmAddress::from(hex_literal::hex!("337610d27c682E347C9cD60BD4b3b107C9d34dDd")),
				EvmAddress::from(hex_literal::hex!("cA2Fc8ABb5ACEc1CA19c684BdF2959B32e83bacF")),
				EvmAddress::from(hex_literal::hex!("3362FD7D8264387Ac7D686084CBB774bB09732DF")),
				EvmAddress::from(hex_literal::hex!("6b5A4f429aAA2E049919b69D95f2A26bef01912C")),
			),
			_ => (
				343u64, // localnet Bsc Chain ID
				EvmAddress::from(hex_literal::hex!("Dc64a140Aa3E981100a9becA4E685f962f0cF6C9")),
				EvmAddress::from(hex_literal::hex!("5FbDB2315678afecb367f032d93F642f64180aa3")),
				EvmAddress::from(hex_literal::hex!("e7f1725E7734CE288F8367e1Bb143E90bb3F0512")),
				EvmAddress::from(hex_literal::hex!("9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0")),
			),
		};

		BscChainId::<T>::set(chain_id);
		BscSupportedAssets::<T>::insert(BscAsset::BscUsdt, bsc_usdt_address);
		BscKeyManagerAddress::<T>::set(key_manager_address);
		BscVaultAddress::<T>::set(vault_address);
		BscAddressCheckerAddress::<T>::set(address_checker_address);

		log::info!("🔧 Environment pallet Bsc migration completed.");

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		match cf_runtime_utilities::genesis_hashes::genesis_hash::<T>() {
			cf_runtime_utilities::genesis_hashes::BERGHAIN => {
				assert_eq!(BscChainId::<T>::get(), cf_chains::bsc::CHAIN_ID_MAINNET);
				assert_eq!(
					BscSupportedAssets::<T>::get(BscAsset::BscUsdt),
					Some(EvmAddress::from(hex_literal::hex!(
						"55d398326f99059fF775485246999027B3197955"
					)))
				);
				assert_eq!(
					BscKeyManagerAddress::<T>::get(),
					EvmAddress::from(hex_literal::hex!("BFe612c77C2807Ac5a6A41F84436287578000275"))
				);
				assert_eq!(
					BscVaultAddress::<T>::get(),
					EvmAddress::from(hex_literal::hex!("79001a5e762f3bEFC8e5871b42F6734e00498920"))
				);
				assert_eq!(
					BscAddressCheckerAddress::<T>::get(),
					EvmAddress::from(hex_literal::hex!("c1B12993f760B654897F0257573202fba13D5481"))
				);
			},
			cf_runtime_utilities::genesis_hashes::PERSEVERANCE => {
				assert_eq!(BscChainId::<T>::get(), cf_chains::bsc::CHAIN_ID_TESTNET);
				assert_eq!(
					BscSupportedAssets::<T>::get(BscAsset::BscUsdt),
					Some(EvmAddress::from(hex_literal::hex!(
						"337610d27c682E347C9cD60BD4b3b107C9d34dDd"
					)))
				);
				assert_eq!(
					BscKeyManagerAddress::<T>::get(),
					EvmAddress::from(hex_literal::hex!("77864880BA7D2F8A95d540D85233A250EcfafDc0"))
				);
				assert_eq!(
					BscVaultAddress::<T>::get(),
					EvmAddress::from(hex_literal::hex!("98b9829Cc96e910B1253163E708e4cBF3F5BE277"))
				);
				assert_eq!(
					BscAddressCheckerAddress::<T>::get(),
					EvmAddress::from(hex_literal::hex!("0fDA3D36ce05531F1cb14E519672dd52C314Fd28"))
				);
			},
			cf_runtime_utilities::genesis_hashes::SISYPHOS => {
				assert_eq!(BscChainId::<T>::get(), cf_chains::bsc::CHAIN_ID_TESTNET);
				assert_eq!(
					BscSupportedAssets::<T>::get(BscAsset::BscUsdt),
					Some(EvmAddress::from(hex_literal::hex!(
						"337610d27c682E347C9cD60BD4b3b107C9d34dDd"
					)))
				);
				assert_eq!(
					BscKeyManagerAddress::<T>::get(),
					EvmAddress::from(hex_literal::hex!("cA2Fc8ABb5ACEc1CA19c684BdF2959B32e83bacF"))
				);
				assert_eq!(
					BscVaultAddress::<T>::get(),
					EvmAddress::from(hex_literal::hex!("3362FD7D8264387Ac7D686084CBB774bB09732DF"))
				);
				assert_eq!(
					BscAddressCheckerAddress::<T>::get(),
					EvmAddress::from(hex_literal::hex!("6b5A4f429aAA2E049919b69D95f2A26bef01912C"))
				);
			},
			_ => {
				assert_eq!(BscChainId::<T>::get(), 343);
				assert_eq!(
					BscSupportedAssets::<T>::get(BscAsset::BscUsdt),
					Some(EvmAddress::from(hex_literal::hex!(
						"Dc64a140Aa3E981100a9becA4E685f962f0cF6C9"
					)))
				);
				assert_eq!(
					BscKeyManagerAddress::<T>::get(),
					EvmAddress::from(hex_literal::hex!("5FbDB2315678afecb367f032d93F642f64180aa3"))
				);
				assert_eq!(
					BscVaultAddress::<T>::get(),
					EvmAddress::from(hex_literal::hex!("e7f1725E7734CE288F8367e1Bb143E90bb3F0512"))
				);
				assert_eq!(
					BscAddressCheckerAddress::<T>::get(),
					EvmAddress::from(hex_literal::hex!("9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0"))
				);
			},
		};
		Ok(())
	}
}
