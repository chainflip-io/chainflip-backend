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
			// TODO: Replace with actual mainnet addresses
			cf_runtime_utilities::genesis_hashes::BERGHAIN => (
				cf_chains::bsc::CHAIN_ID_MAINNET,
				EvmAddress::default(),
				EvmAddress::default(),
				EvmAddress::default(),
				EvmAddress::default(),
			),
			// TODO: Replace with actual PERSEVERANCE addresses
			cf_runtime_utilities::genesis_hashes::PERSEVERANCE => (
				cf_chains::bsc::CHAIN_ID_TESTNET,
				EvmAddress::default(),
				EvmAddress::default(),
				EvmAddress::default(),
				EvmAddress::default(),
			),
			// TODO: Replace with actual SISYPHOS addresses
			cf_runtime_utilities::genesis_hashes::SISYPHOS => (
				cf_chains::bsc::CHAIN_ID_TESTNET,
				EvmAddress::default(),
				EvmAddress::default(),
				EvmAddress::default(),
				EvmAddress::default(),
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
					Some(EvmAddress::default()) // TODO: update when real address is known
				);
				// TODO: update when real addresses are known
				assert_eq!(BscKeyManagerAddress::<T>::get(), EvmAddress::default());
				assert_eq!(BscVaultAddress::<T>::get(), EvmAddress::default());
				assert_eq!(BscAddressCheckerAddress::<T>::get(), EvmAddress::default());
			},
			cf_runtime_utilities::genesis_hashes::PERSEVERANCE => {
				assert_eq!(BscChainId::<T>::get(), cf_chains::bsc::CHAIN_ID_TESTNET);
				assert_eq!(
					BscSupportedAssets::<T>::get(BscAsset::BscUsdt),
					Some(EvmAddress::default()) // TODO: update when real address is known
				);
				// TODO: update when real addresses are known
				assert_eq!(BscKeyManagerAddress::<T>::get(), EvmAddress::default());
				assert_eq!(BscVaultAddress::<T>::get(), EvmAddress::default());
				assert_eq!(BscAddressCheckerAddress::<T>::get(), EvmAddress::default());
			},
			cf_runtime_utilities::genesis_hashes::SISYPHOS => {
				assert_eq!(BscChainId::<T>::get(), cf_chains::bsc::CHAIN_ID_TESTNET);
				assert_eq!(
					BscSupportedAssets::<T>::get(BscAsset::BscUsdt),
					Some(EvmAddress::default()) // TODO: update when real address is known
				);
				// TODO: update when real addresses are known
				assert_eq!(BscKeyManagerAddress::<T>::get(), EvmAddress::default());
				assert_eq!(BscVaultAddress::<T>::get(), EvmAddress::default());
				assert_eq!(BscAddressCheckerAddress::<T>::get(), EvmAddress::default());
			},
			_ => {
				assert_eq!(BscChainId::<T>::get(), 343);
				assert!(BscSupportedAssets::<T>::get(BscAsset::BscUsdt).is_some());
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
