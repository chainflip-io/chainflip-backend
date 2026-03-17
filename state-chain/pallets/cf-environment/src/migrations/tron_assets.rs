use crate::*;

use cf_chains::evm::{Address as EvmAddress, H256};
use frame_support::{pallet_prelude::Weight, traits::UncheckedOnRuntimeUpgrade};

pub struct TronAssetsMigration<T>(PhantomData<T>);

impl<T: Config<Hash = H256>> UncheckedOnRuntimeUpgrade for TronAssetsMigration<T> {
	fn on_runtime_upgrade() -> Weight {
		log::info!("🔧 Running Environment pallet Tron migration...");

		let (chain_id, tron_usdt_address, key_manager_address, vault_address) =
			match cf_runtime_utilities::genesis_hashes::genesis_hash::<T>() {
				// TODO: Replace with actual mainnet addresses
				cf_runtime_utilities::genesis_hashes::BERGHAIN => (
					cf_chains::tron::CHAIN_ID_MAINNET,
					EvmAddress::default(),
					EvmAddress::default(),
					EvmAddress::default(),
				),
				// TODO: Replace with actual PERSEVERANCE addresses
				cf_runtime_utilities::genesis_hashes::PERSEVERANCE => (
					cf_chains::tron::CHAIN_ID_NILE_TESTNET,
					EvmAddress::default(),
					EvmAddress::default(),
					EvmAddress::default(),
				),
				// TODO: Replace with actual SISYPHOS addresses
				cf_runtime_utilities::genesis_hashes::SISYPHOS => (
					cf_chains::tron::CHAIN_ID_NILE_TESTNET,
					EvmAddress::default(),
					EvmAddress::default(),
					EvmAddress::default(),
				),
				_ => (
					4271970548u64, // localnet Tron Chain ID
					// TODO: Set actual addresses once we have a final localnet image
					EvmAddress::from(hex_literal::hex!("874dabe2d179dd5060b48881872b5851458c9fe0")),
					EvmAddress::from(hex_literal::hex!("9df3e70fc7ea8128d6d0634664118d16bc856e1c")),
					EvmAddress::from(hex_literal::hex!("a0e915df8a24a4718061461a41f303c6d3353e51")),
				),
			};

		TronChainId::<T>::set(chain_id);
		TronSupportedAssets::<T>::insert(TrxAsset::TronUsdt, tron_usdt_address);
		TronKeyManagerAddress::<T>::set(key_manager_address);
		TronVaultAddress::<T>::set(vault_address);

		log::info!("🔧 Environment pallet Tron migration completed.");

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		match cf_runtime_utilities::genesis_hashes::genesis_hash::<T>() {
			cf_runtime_utilities::genesis_hashes::BERGHAIN => {
				assert_eq!(TronChainId::<T>::get(), cf_chains::tron::CHAIN_ID_MAINNET);
				assert_eq!(
					TronSupportedAssets::<T>::get(TrxAsset::TronUsdt),
					Some(EvmAddress::default()) // TODO: update when real address is known
				);
				// TODO: update when real addresses are known
				assert_eq!(TronKeyManagerAddress::<T>::get(), EvmAddress::default());
				assert_eq!(TronVaultAddress::<T>::get(), EvmAddress::default());
			},
			cf_runtime_utilities::genesis_hashes::PERSEVERANCE => {
				assert_eq!(TronChainId::<T>::get(), cf_chains::tron::CHAIN_ID_NILE_TESTNET);
				assert_eq!(
					TronSupportedAssets::<T>::get(TrxAsset::TronUsdt),
					Some(EvmAddress::default()) // TODO: update when real address is known
				);
				// TODO: update when real addresses are known
				assert_eq!(TronKeyManagerAddress::<T>::get(), EvmAddress::default());
				assert_eq!(TronVaultAddress::<T>::get(), EvmAddress::default());
			},
			cf_runtime_utilities::genesis_hashes::SISYPHOS => {
				assert_eq!(TronChainId::<T>::get(), cf_chains::tron::CHAIN_ID_NILE_TESTNET);
				assert_eq!(
					TronSupportedAssets::<T>::get(TrxAsset::TronUsdt),
					Some(EvmAddress::default()) // TODO: update when real address is known
				);
				// TODO: update when real addresses are known
				assert_eq!(TronKeyManagerAddress::<T>::get(), EvmAddress::default());
				assert_eq!(TronVaultAddress::<T>::get(), EvmAddress::default());
			},
			_ => {
				assert_eq!(TronChainId::<T>::get(), 4271970548);
				assert!(TronSupportedAssets::<T>::get(TrxAsset::TronUsdt).is_some());
				assert_eq!(
					TronKeyManagerAddress::<T>::get(),
					EvmAddress::from(hex_literal::hex!("9df3e70fc7ea8128d6d0634664118d16bc856e1c"))
				);
				assert_eq!(
					TronVaultAddress::<T>::get(),
					EvmAddress::from(hex_literal::hex!("a0e915df8a24a4718061461a41f303c6d3353e51"))
				);
			},
		};
		Ok(())
	}
}
