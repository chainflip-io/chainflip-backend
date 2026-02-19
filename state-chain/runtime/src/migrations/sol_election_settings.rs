use crate::*;

use crate::chainflip::witnessing::solana_elections::{
	SolanaIngressSettings, SolanaVaultSwapsSettings,
};
use cf_chains::sol::SolAddress;
use cf_primitives::NetworkEnvironment;
use cf_runtime_utilities::NoopRuntimeUpgrade;
use cf_utilities::bs58_array;
use frame_support::{
	migrations::VersionedMigration, traits::UncheckedOnRuntimeUpgrade, weights::Weight,
};
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

pub type Migration = (
	VersionedMigration<
		7,
		8,
		NoopRuntimeUpgrade,
		pallet_cf_elections::Pallet<Runtime, ()>,
		<Runtime as frame_system::Config>::DbWeight,
	>,
	VersionedMigration<
		7,
		8,
		NoopRuntimeUpgrade,
		pallet_cf_elections::Pallet<Runtime, BitcoinInstance>,
		<Runtime as frame_system::Config>::DbWeight,
	>,
	VersionedMigration<
		7,
		8,
		SolElectionSettingsMigration,
		pallet_cf_elections::Pallet<Runtime, SolanaInstance>,
		<Runtime as frame_system::Config>::DbWeight,
	>,
);

pub struct SolElectionSettingsMigration;

mod old {
	use crate::runtime_apis::types::{Deserialize, Serialize};
	use cf_chains::sol::SolAddress;
	use cf_primitives::BlockNumber;
	use frame_support::{pallet_prelude::OptionQuery, Twox64Concat};
	use pallet_cf_elections::{
		electoral_systems::blockchain::delta_based_ingress::BackoffSettings,
		generic_tools::{Decode, Encode, TypeInfo},
		Config, Pallet, UniqueMonotonicIdentifier,
	};

	#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
	pub struct OldSolanaIngressSettings {
		pub vault_program: SolAddress,
		pub usdc_token_mint_pubkey: SolAddress,
	}

	#[derive(
		Clone,
		PartialEq,
		Eq,
		Debug,
		Serialize,
		Deserialize,
		TypeInfo,
		Encode,
		Decode,
		PartialOrd,
		Ord,
	)]
	pub struct OldSolanaVaultSwapsSettings {
		pub swap_endpoint_data_account_address: SolAddress,
		pub usdc_token_mint_pubkey: SolAddress,
	}

	pub type CompositeElectoralSettings = (
		(),
		(OldSolanaIngressSettings, BackoffSettings<BlockNumber>),
		(),
		(),
		BlockNumber,
		OldSolanaVaultSwapsSettings,
		(),
	);

	#[frame_support::storage_alias]
	pub type ElectoralSettings<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Twox64Concat,
		UniqueMonotonicIdentifier,
		CompositeElectoralSettings,
		OptionQuery,
	>;
}

impl UncheckedOnRuntimeUpgrade for SolElectionSettingsMigration {
	fn on_runtime_upgrade() -> Weight {
		log::info!("🍩 Migration for Solana ElectoralSettings started");

		let solusdt_pubkey =
			match pallet_cf_environment::ChainflipNetworkEnvironment::<Runtime>::get() {
				NetworkEnvironment::Mainnet =>
					SolAddress(bs58_array("Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB")),
				NetworkEnvironment::Testnet =>
					SolAddress(bs58_array("FvuqJYh8YeEmarW5qkSrYeEgzaTKktgL3vhgBy2Csy4o")),
				NetworkEnvironment::Development =>
					SolAddress(bs58_array("8D5DryH5hA6s7Wf5AHXX19pNBwaTmMmvj4UgQGW2S8dF")),
			};

		let old_settings_entries: Vec<_> =
			old::ElectoralSettings::<Runtime, SolanaInstance>::drain().collect();

		for (id, (a, (ingress_settings, b), c, d, e, vault_swap_settings, f)) in
			old_settings_entries
		{
			pallet_cf_elections::ElectoralSettings::<Runtime, SolanaInstance>::insert(
				id,
				(
					a,
					(
						SolanaIngressSettings {
							vault_program: ingress_settings.vault_program,
							usdc_token_mint_pubkey: ingress_settings.usdc_token_mint_pubkey,
							usdt_token_mint_pubkey: solusdt_pubkey,
						},
						b,
					),
					c,
					d,
					e,
					SolanaVaultSwapsSettings {
						swap_endpoint_data_account_address: vault_swap_settings
							.swap_endpoint_data_account_address,
						usdc_token_mint_pubkey: vault_swap_settings.usdc_token_mint_pubkey,
						usdt_token_mint_pubkey: solusdt_pubkey,
					},
					f,
				),
			);
		}

		log::info!("🍩 Migrating Solana ElectoralSettings finished");

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let settings_entries: Vec<_> =
			old::ElectoralSettings::<Runtime, SolanaInstance>::iter().collect();

		Ok(settings_entries.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		use pallet_cf_elections::UniqueMonotonicIdentifier;

		let old_settings_entries = Vec::<(
			UniqueMonotonicIdentifier,
			old::CompositeElectoralSettings,
		)>::decode(&mut state.as_slice())
		.map_err(|_| DispatchError::from("Failed to decode state"))?;

		let solusdt_pubkey =
			match pallet_cf_environment::ChainflipNetworkEnvironment::<Runtime>::get() {
				NetworkEnvironment::Mainnet =>
					SolAddress(bs58_array("Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB")),
				NetworkEnvironment::Testnet =>
					SolAddress(bs58_array("FvuqJYh8YeEmarW5qkSrYeEgzaTKktgL3vhgBy2Csy4o")),
				NetworkEnvironment::Development =>
					SolAddress(bs58_array("8D5DryH5hA6s7Wf5AHXX19pNBwaTmMmvj4UgQGW2S8dF")),
			};

		for (id, (_a, (ingress_settings, b), _c, _d, e, vault_swap_settings, _f)) in
			old_settings_entries
		{
			let new_entry =
				pallet_cf_elections::ElectoralSettings::<Runtime, SolanaInstance>::get(id)
					.ok_or(DispatchError::from("Failed to fetch entry"))?;

			assert_eq!(new_entry.1 .0.vault_program, ingress_settings.vault_program);
			assert_eq!(
				new_entry.1 .0.usdc_token_mint_pubkey,
				ingress_settings.usdc_token_mint_pubkey
			);
			assert_eq!(new_entry.1 .0.usdt_token_mint_pubkey, solusdt_pubkey);
			assert_eq!(new_entry.1 .1, b);

			assert_eq!(new_entry.4, e);
			assert_eq!(
				new_entry.5.swap_endpoint_data_account_address,
				vault_swap_settings.swap_endpoint_data_account_address
			);
			assert_eq!(
				new_entry.5.usdc_token_mint_pubkey,
				vault_swap_settings.usdc_token_mint_pubkey
			);
			assert_eq!(new_entry.5.usdt_token_mint_pubkey, solusdt_pubkey);
		}

		Ok(())
	}
}
