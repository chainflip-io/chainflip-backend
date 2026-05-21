use crate::*;

use cf_chains::evm::{Address as EvmAddress, H256};
use frame_support::{pallet_prelude::Weight, traits::UncheckedOnRuntimeUpgrade};

pub struct TronAssetsMigration<T>(PhantomData<T>);

impl<T: Config<Hash = H256>> UncheckedOnRuntimeUpgrade for TronAssetsMigration<T> {
	fn on_runtime_upgrade() -> Weight {
		log::info!("🔧 Running Environment pallet Tron migration...");

		let (chain_id, trx_usdt_address, key_manager_address, vault_address) =
			match cf_runtime_utilities::genesis_hashes::genesis_hash::<T>() {
				cf_runtime_utilities::genesis_hashes::BERGHAIN => (
					cf_chains::tron::CHAIN_ID_MAINNET,
					EvmAddress::from(hex_literal::hex!("a614f803b6fd780986a42c78ec9c7f77e6ded13c")), /* TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t */
					EvmAddress::from(hex_literal::hex!("67d11affdff89ba412e60ecfc8db778950486039")), /* TKS96iibRRLvLHL6cDNyTfbziC2vBPcFvB */
					EvmAddress::from(hex_literal::hex!("24c97d696e0db753b229be451d1a50705a09566e")), /* TDKihjArLMEfF8ebeEWdfiuoqHC8nmydBy */
				),
				cf_runtime_utilities::genesis_hashes::PERSEVERANCE => (
					cf_chains::tron::CHAIN_ID_NILE_TESTNET,
					EvmAddress::from(hex_literal::hex!("eca9bc828a3005b9a3b909f2cc5c2a54794de05f")), /* TXYZopYRdj2D9XRtbG411XZZ3kM5VkAeBf */
					EvmAddress::from(hex_literal::hex!("391362ed6f5f649868eede354d3e833d6165dd9a")), /* TFAzjzAnaFWRmXNvpNngTbBBDuo4WjRQyy */
					EvmAddress::from(hex_literal::hex!("2523ae929fecd9d665f472f59b99a8ce6b179510")), /* TDMakP1fbWc7XXoSWZpujpjRAuePPEn4oi */
				),
				cf_runtime_utilities::genesis_hashes::SISYPHOS => (
					cf_chains::tron::CHAIN_ID_NILE_TESTNET,
					EvmAddress::from(hex_literal::hex!("eca9bc828a3005b9a3b909f2cc5c2a54794de05f")), /* TXYZopYRdj2D9XRtbG411XZZ3kM5VkAeBf */
					EvmAddress::from(hex_literal::hex!("fa989a8351b3ada8f871d12a75635d1589cf74a0")), /* TYpEk6eqy6Y7FXydzqMb5nvWfDy8Bs8qiT */
					EvmAddress::from(hex_literal::hex!("95b740ec67963cc17ef1931978639e320c0b4be5")), /* TPcqDKqWiDyQh4vGbFiJaxpPXtKfk8RZML */
				),
				_ => (
					4271970548u64, // localnet Tron Chain ID
					EvmAddress::from(hex_literal::hex!("a24205e83c1805fcdae7a93db837dc6a0b843340")),
					EvmAddress::from(hex_literal::hex!("f7229affc93ee042e750ce66a062b381f393b25a")),
					EvmAddress::from(hex_literal::hex!("814f36c1bbfd8aabec86273f0d61521e0c2d5287")),
				),
			};

		TronChainId::<T>::set(chain_id);
		TronSupportedAssets::<T>::insert(TrxAsset::TrxUsdt, trx_usdt_address);
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
					TronSupportedAssets::<T>::get(TrxAsset::TrxUsdt),
					Some(EvmAddress::from(hex_literal::hex!(
						"a614f803b6fd780986a42c78ec9c7f77e6ded13c"
					)))
				);
				assert_eq!(
					TronKeyManagerAddress::<T>::get(),
					EvmAddress::from(hex_literal::hex!("67d11affdff89ba412e60ecfc8db778950486039"))
				);
				assert_eq!(
					TronVaultAddress::<T>::get(),
					EvmAddress::from(hex_literal::hex!("24c97d696e0db753b229be451d1a50705a09566e"))
				);
			},
			cf_runtime_utilities::genesis_hashes::PERSEVERANCE => {
				assert_eq!(TronChainId::<T>::get(), cf_chains::tron::CHAIN_ID_NILE_TESTNET);
				assert_eq!(
					TronSupportedAssets::<T>::get(TrxAsset::TrxUsdt),
					Some(EvmAddress::from(hex_literal::hex!(
						"eca9bc828a3005b9a3b909f2cc5c2a54794de05f"
					)))
				);
				assert_eq!(
					TronKeyManagerAddress::<T>::get(),
					EvmAddress::from(hex_literal::hex!("391362ed6f5f649868eede354d3e833d6165dd9a"))
				);
				assert_eq!(
					TronVaultAddress::<T>::get(),
					EvmAddress::from(hex_literal::hex!("2523ae929fecd9d665f472f59b99a8ce6b179510"))
				);
			},
			cf_runtime_utilities::genesis_hashes::SISYPHOS => {
				assert_eq!(TronChainId::<T>::get(), cf_chains::tron::CHAIN_ID_NILE_TESTNET);
				assert_eq!(
					TronSupportedAssets::<T>::get(TrxAsset::TrxUsdt),
					Some(EvmAddress::from(hex_literal::hex!(
						"eca9bc828a3005b9a3b909f2cc5c2a54794de05f"
					)))
				);
				assert_eq!(
					TronKeyManagerAddress::<T>::get(),
					EvmAddress::from(hex_literal::hex!("fa989a8351b3ada8f871d12a75635d1589cf74a0"))
				);
				assert_eq!(
					TronVaultAddress::<T>::get(),
					EvmAddress::from(hex_literal::hex!("95b740ec67963cc17ef1931978639e320c0b4be5"))
				);
			},
			_ => {
				assert_eq!(TronChainId::<T>::get(), 4271970548);
				assert_eq!(
					TronSupportedAssets::<T>::get(TrxAsset::TrxUsdt),
					Some(EvmAddress::from(hex_literal::hex!(
						"a24205e83c1805fcdae7a93db837dc6a0b843340"
					)))
				);
				assert_eq!(
					TronKeyManagerAddress::<T>::get(),
					EvmAddress::from(hex_literal::hex!("f7229affc93ee042e750ce66a062b381f393b25a"))
				);
				assert_eq!(
					TronVaultAddress::<T>::get(),
					EvmAddress::from(hex_literal::hex!("814f36c1bbfd8aabec86273f0d61521e0c2d5287"))
				);
			},
		};
		Ok(())
	}
}
