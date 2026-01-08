use crate::*;

use cf_chains::{eth::Address as EthereumAddress, evm::H256};
use frame_support::{pallet_prelude::Weight, traits::UncheckedOnRuntimeUpgrade};
pub struct NewAssetsMigration<T>(PhantomData<T>);

impl<T: Config<Hash = H256>> UncheckedOnRuntimeUpgrade for NewAssetsMigration<T> {
	fn on_runtime_upgrade() -> Weight {
		log::info!("🌮 Running migration for Environment pallet: Adding WBTC to EthereumSupportedAssets ...");

		// TODO ADD deployed addresses for testnet
		let wbtc_address: EthereumAddress = match ChainflipNetworkEnvironment::<T>::get() {
			NetworkEnvironment::Mainnet =>
				hex_literal::hex!("2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599").into(),
			NetworkEnvironment::Testnet =>
				hex_literal::hex!("b060796D171EeEdA5Fb99df6B2847DA6D4613CAd").into(),
			NetworkEnvironment::Development =>
				hex_literal::hex!("B7f8BC63BbcaD18155201308C8f3540b07f84F5e").into(),
		};
		EthereumSupportedAssets::<T>::insert(EthAsset::Wbtc, wbtc_address);

		// TODO ADD deployed addresses for testnet
		let arbusdt_address: EthereumAddress = match ChainflipNetworkEnvironment::<T>::get() {
			NetworkEnvironment::Mainnet =>
				hex_literal::hex!("Fd086bC7CD5C481DCC9C85ebE478A1C0b69FCbb9").into(),
			NetworkEnvironment::Testnet =>
				hex_literal::hex!("5FC8d32690cc91D4c39d9d3abcBD16989F875707").into(),
			NetworkEnvironment::Development =>
				hex_literal::hex!("5FC8d32690cc91D4c39d9d3abcBD16989F875707").into(),
		};
		ArbitrumSupportedAssets::<T>::insert(ArbAsset::ArbUsdt, arbusdt_address);

		log::info!(
			"🌮 Environment pallet migration completed: Added WBTC to EthereumSupportedAssets."
		);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		let wbtc_address = EthereumSupportedAssets::<T>::get(EthAsset::Wbtc)
			.ok_or(DispatchError::from("Failed to fetch WBTC address"))?;

		match ChainflipNetworkEnvironment::<T>::get() {
			NetworkEnvironment::Mainnet => assert_eq!(
				wbtc_address,
				hex_literal::hex!("2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599").into()
			),
			NetworkEnvironment::Testnet => assert_eq!(
				wbtc_address,
				hex_literal::hex!("b060796D171EeEdA5Fb99df6B2847DA6D4613CAd").into()
			),
			NetworkEnvironment::Development => assert_eq!(
				wbtc_address,
				hex_literal::hex!("B7f8BC63BbcaD18155201308C8f3540b07f84F5e").into()
			),
		};

		let arbusdt_address = ArbitrumSupportedAssets::<T>::get(ArbAsset::ArbUsdt)
			.ok_or(DispatchError::from("Failed to fetch ArbUsdt address"))?;

		match ChainflipNetworkEnvironment::<T>::get() {
			NetworkEnvironment::Mainnet => assert_eq!(
				arbusdt_address,
				hex_literal::hex!("Fd086bC7CD5C481DCC9C85ebE478A1C0b69FCbb9").into()
			),
			NetworkEnvironment::Testnet => assert_eq!(
				arbusdt_address,
				hex_literal::hex!("5FC8d32690cc91D4c39d9d3abcBD16989F875707").into()
			),
			NetworkEnvironment::Development => assert_eq!(
				arbusdt_address,
				hex_literal::hex!("5FC8d32690cc91D4c39d9d3abcBD16989F875707").into()
			),
		};

		Ok(())
	}
}
