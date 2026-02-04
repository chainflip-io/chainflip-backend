use crate::*;

use cf_chains::{eth::Address as EthereumAddress, evm::H256};
use cf_utilities::bs58_array;
use frame_support::{pallet_prelude::Weight, traits::UncheckedOnRuntimeUpgrade};

mod old {
	use crate::{pallet::Config, Pallet};
	use cf_chains::sol::{AddressLookupTableAccount, SolAddress};
	use codec::{Decode, Encode};
	use frame_support::{
		pallet_prelude::{TypeInfo, ValueQuery},
		Deserialize, Serialize,
	};

	#[derive(
		Encode, Decode, TypeInfo, Default, Clone, PartialEq, Eq, Debug, Serialize, Deserialize,
	)]
	pub struct OldSolApiEnvironment {
		// For native Sol API calls.
		pub vault_program: SolAddress,
		pub vault_program_data_account: SolAddress,

		// For token API calls.
		pub token_vault_pda_account: SolAddress,

		// For Usdc token
		pub usdc_token_mint_pubkey: SolAddress,
		pub usdc_token_vault_ata: SolAddress,

		// For program swaps API calls.
		pub swap_endpoint_program: SolAddress,
		pub swap_endpoint_program_data_account: SolAddress,
		pub alt_manager_program: SolAddress,
		pub address_lookup_table_account: AddressLookupTableAccount,
	}

	#[frame_support::storage_alias]
	pub type SolanaApiEnvironment<T: Config> =
		StorageValue<Pallet<T>, OldSolApiEnvironment, ValueQuery>;
}

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

		let old_sol_api_environment = old::SolanaApiEnvironment::<T>::take();
		SolanaApiEnvironment::<T>::set(SolApiEnvironment {
			vault_program: old_sol_api_environment.vault_program,
			vault_program_data_account: old_sol_api_environment.vault_program_data_account,
			token_vault_pda_account: old_sol_api_environment.token_vault_pda_account,
			usdc_token_mint_pubkey: old_sol_api_environment.usdc_token_mint_pubkey,
			usdc_token_vault_ata: old_sol_api_environment.usdc_token_vault_ata,
			usdt_token_mint_pubkey: solusdt_pubkey,
			usdt_token_vault_ata: solusdt_ata,
			swap_endpoint_program: old_sol_api_environment.swap_endpoint_program,
			swap_endpoint_program_data_account: old_sol_api_environment
				.swap_endpoint_program_data_account,
			alt_manager_program: old_sol_api_environment.alt_manager_program,
			address_lookup_table_account: old_sol_api_environment.address_lookup_table_account,
		});

		log::info!("🌮 Environment pallet migration completed: Added Wbtc, ArbUsdt and SolUsdt");

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let old_sol_api_environment = old::SolanaApiEnvironment::<T>::get();
		Ok(old_sol_api_environment.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
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

		let old_sol_api_environment = old::OldSolApiEnvironment::decode(&mut state.as_slice())
			.map_err(|_| DispatchError::from("Failed to decode state"))?;
		let solana_api_environment = SolanaApiEnvironment::<T>::get();

		assert_eq!(solana_api_environment.usdt_token_mint_pubkey, expected_solusdt_pubkey);
		assert_eq!(solana_api_environment.usdt_token_vault_ata, expected_solusdt_ata);

		assert_eq!(solana_api_environment.vault_program, old_sol_api_environment.vault_program);
		assert_eq!(
			solana_api_environment.vault_program_data_account,
			old_sol_api_environment.vault_program_data_account
		);
		assert_eq!(
			solana_api_environment.token_vault_pda_account,
			old_sol_api_environment.token_vault_pda_account
		);
		assert_eq!(
			solana_api_environment.usdc_token_mint_pubkey,
			old_sol_api_environment.usdc_token_mint_pubkey
		);
		assert_eq!(
			solana_api_environment.usdc_token_vault_ata,
			old_sol_api_environment.usdc_token_vault_ata
		);
		assert_eq!(
			solana_api_environment.swap_endpoint_program,
			old_sol_api_environment.swap_endpoint_program
		);
		assert_eq!(
			solana_api_environment.swap_endpoint_program_data_account,
			old_sol_api_environment.swap_endpoint_program_data_account
		);
		assert_eq!(
			solana_api_environment.alt_manager_program,
			old_sol_api_environment.alt_manager_program
		);
		assert_eq!(
			solana_api_environment.address_lookup_table_account,
			old_sol_api_environment.address_lookup_table_account
		);

		Ok(())
	}
}
