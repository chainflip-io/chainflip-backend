use crate::*;

use cf_chains::{eth::Address as EthereumAddress, evm::H256};
use cf_utilities::bs58_array;
use frame_support::{pallet_prelude::Weight, traits::UncheckedOnRuntimeUpgrade};

pub struct NewAssetsMigration<T>(PhantomData<T>);

impl<T: Config<Hash = H256>> UncheckedOnRuntimeUpgrade for NewAssetsMigration<T> {
	fn on_runtime_upgrade() -> Weight {
		log::info!("🌮 Running migration for Environment pallet: Adding Wbtc to EthereumSupportedAssets ...");
		let wbtc_address: EthereumAddress = match ChainflipNetworkEnvironment::<T>::get() {
			NetworkEnvironment::Mainnet =>
				hex_literal::hex!("2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599").into(),
			NetworkEnvironment::Testnet =>
				hex_literal::hex!("aaf48bd21155efeff9ca3699659c96bc86539b49").into(),
			NetworkEnvironment::Development =>
				hex_literal::hex!("67d269191c92Caf3cD7723F116c85e6E9bf55933").into(),
		};
		EthereumSupportedAssets::<T>::insert(EthAsset::Wbtc, wbtc_address);

		log::info!("🌮 Running migration for Environment pallet: Adding ArbUsdt to ArbitrumSupportedAssets ...");
		let arbusdt_address: EthereumAddress = match ChainflipNetworkEnvironment::<T>::get() {
			NetworkEnvironment::Mainnet =>
				hex_literal::hex!("Fd086bC7CD5C481DCC9C85ebE478A1C0b69FCbb9").into(),
			NetworkEnvironment::Testnet =>
				hex_literal::hex!("3dd1A7A99CFa2554Da8b3483e6eD739120Fc35cB").into(),
			NetworkEnvironment::Development =>
				hex_literal::hex!("9A676e781A523b5d0C0e43731313A708CB607508").into(),
		};
		ArbitrumSupportedAssets::<T>::insert(ArbAsset::ArbUsdt, arbusdt_address);

		log::info!("🌮 Running migration for Environment pallet: Adding SolUsdt to SolanaApiEnvironment ...");
		let (solusdt_pubkey, solusdt_ata) = match ChainflipNetworkEnvironment::<T>::get() {
			NetworkEnvironment::Mainnet => (
				SolAddress(bs58_array("Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB")),
				SolAddress(bs58_array("R4Z9JGY8iMmG4UJf6aBtzobAo4rmGMdfLgXrbXSWc4y")),
			),
			NetworkEnvironment::Testnet => (
				SolAddress(bs58_array("FvuqJYh8YeEmarW5qkSrYeEgzaTKktgL3vhgBy2Csy4o")),
				SolAddress(bs58_array("FwJtRJ3zRQS7wQmjhvxShvGoUSAyaasBcoEJxybbVbEJ")),
			),
			NetworkEnvironment::Development => (
				SolAddress(bs58_array("8D5DryH5hA6s7Wf5AHXX19pNBwaTmMmvj4UgQGW2S8dF")),
				SolAddress(bs58_array("FjAhVXJj9N7nqVK6y5fzgVcAxtRYFnHCZgy9FEMMDww3")),
			),
		};

		SolanaApiEnvironment::<T>::mutate(|sol_api_environment| {
			sol_api_environment.usdt_token_mint_pubkey = solusdt_pubkey;
			sol_api_environment.usdt_token_vault_ata = solusdt_ata;
		});

		log::info!("🌮 Environment pallet migration completed: Added Wbtc, ArbUsdt and SolUsdt");

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
				hex_literal::hex!("aaf48bd21155efeff9ca3699659c96bc86539b49").into()
			),
			NetworkEnvironment::Development => assert_eq!(
				wbtc_address,
				hex_literal::hex!("67d269191c92Caf3cD7723F116c85e6E9bf55933").into()
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
				hex_literal::hex!("3dd1A7A99CFa2554Da8b3483e6eD739120Fc35cB").into()
			),
			NetworkEnvironment::Development => assert_eq!(
				arbusdt_address,
				hex_literal::hex!("9A676e781A523b5d0C0e43731313A708CB607508").into()
			),
		};

		let solana_api_environment = SolanaApiEnvironment::<T>::get();

		let (expected_solusdt_pubkey, expected_solusdt_ata) =
			match ChainflipNetworkEnvironment::<T>::get() {
				NetworkEnvironment::Mainnet => (
					SolAddress(bs58_array("Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB")),
					SolAddress(bs58_array("R4Z9JGY8iMmG4UJf6aBtzobAo4rmGMdfLgXrbXSWc4y")),
				),
				NetworkEnvironment::Testnet => (
					SolAddress(bs58_array("FvuqJYh8YeEmarW5qkSrYeEgzaTKktgL3vhgBy2Csy4o")),
					SolAddress(bs58_array("FwJtRJ3zRQS7wQmjhvxShvGoUSAyaasBcoEJxybbVbEJ")),
				),
				NetworkEnvironment::Development => (
					SolAddress(bs58_array("8D5DryH5hA6s7Wf5AHXX19pNBwaTmMmvj4UgQGW2S8dF")),
					SolAddress(bs58_array("FjAhVXJj9N7nqVK6y5fzgVcAxtRYFnHCZgy9FEMMDww3")),
				),
			};

		assert_eq!(solana_api_environment.usdt_token_mint_pubkey, expected_solusdt_pubkey);
		assert_eq!(solana_api_environment.usdt_token_vault_ata, expected_solusdt_ata);

		Ok(())
	}
}
