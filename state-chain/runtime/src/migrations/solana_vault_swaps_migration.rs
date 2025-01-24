use crate::*;
use chainflip::solana_elections::SolanaVaultSwapsSettings;
use frame_support::{pallet_prelude::Weight, storage::unhashed, traits::UncheckedOnRuntimeUpgrade};

use pallet_cf_elections::{
	electoral_system::ElectoralSystemTypes, electoral_system_runner::ElectoralSystemRunner, Config,
	ElectoralSettings, ElectoralUnsynchronisedState,
};
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

use cf_utilities::bs58_array;
use codec::{Decode, Encode};

pub struct SolanaVaultSwapsMigration;

impl UncheckedOnRuntimeUpgrade for SolanaVaultSwapsMigration {
	fn on_runtime_upgrade() -> Weight {
		let mut raw_unsynchronised_state = unhashed::get_raw(&ElectoralUnsynchronisedState::<
			Runtime,
			SolanaInstance,
		>::hashed_key())
		.unwrap();
		raw_unsynchronised_state.extend(0u32.encode());
		ElectoralUnsynchronisedState::<Runtime, SolanaInstance>::put(<<Runtime as Config<SolanaInstance>>::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralUnsynchronisedState::decode(&mut &raw_unsynchronised_state[..]).unwrap());

		let (usdc_token_mint_pubkey, swap_endpoint_data_account_address) =
			match cf_runtime_utilities::genesis_hashes::genesis_hash::<Runtime>() {
				cf_runtime_utilities::genesis_hashes::BERGHAIN => (
					SolAddress(bs58_array("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v")),
					SolAddress(bs58_array("GfGZCo8KmAQvhZofu3Emt66ZfgjKds6ULhps1DAvN8cm")),
				),
				cf_runtime_utilities::genesis_hashes::PERSEVERANCE => (
					SolAddress(bs58_array("4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU")),
					SolAddress(bs58_array("4hD7UM6rQtcqQWtzELvrafpmBYReVXvCpssB6qjY1Sg5")),
				),
				cf_runtime_utilities::genesis_hashes::SISYPHOS => (
					SolAddress(bs58_array("4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU")),
					SolAddress(bs58_array("mYabVW1uMXpGqwgHUBQu4Fg6GT9EMYUzYaGYbi3zgT7")),
				),
				_ => (
					SolAddress(bs58_array("24PNhTaNtomHhoy3fTRaMhAFCRj4uHqhZEEoWrKDbR5p")),
					SolAddress(bs58_array("2tmtGLQcBd11BMiE9B1tAkQXwmPNgR79Meki2Eme4Ec9")),
				),
			};

		for key in ElectoralSettings::<Runtime, SolanaInstance>::iter_keys() {
			let mut raw_storage_at_key = unhashed::get_raw(&ElectoralSettings::<
				Runtime,
				SolanaInstance,
			>::hashed_key_for(key))
			.expect("We just got the keys directly from the storage");
			raw_storage_at_key.extend(
				SolanaVaultSwapsSettings {
					usdc_token_mint_pubkey,
					swap_endpoint_data_account_address,
				}
				.encode(),
			);
			ElectoralSettings::<Runtime, SolanaInstance>::insert(key, <<Runtime as Config<SolanaInstance>>::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralSettings::decode(&mut &raw_storage_at_key[..]).unwrap());
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		assert!(ElectoralUnsynchronisedState::<Runtime, SolanaInstance>::exists());
		assert!(ElectoralSettings::<Runtime, SolanaInstance>::iter_keys().next().is_some());
		Ok(Default::default())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		let (.., last_block_number) =
			ElectoralUnsynchronisedState::<Runtime, SolanaInstance>::get().unwrap();
		assert_eq!(last_block_number, 0u32);
		for (
			..,
			SolanaVaultSwapsSettings { usdc_token_mint_pubkey, swap_endpoint_data_account_address },
		) in ElectoralSettings::<Runtime, SolanaInstance>::iter_values()
		{
			assert_eq!(
				(usdc_token_mint_pubkey, swap_endpoint_data_account_address),
				match cf_runtime_utilities::genesis_hashes::genesis_hash::<Runtime>() {
					cf_runtime_utilities::genesis_hashes::BERGHAIN => (
						SolAddress(bs58_array("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v")),
						SolAddress(bs58_array("GfGZCo8KmAQvhZofu3Emt66ZfgjKds6ULhps1DAvN8cm")),
					),
					cf_runtime_utilities::genesis_hashes::PERSEVERANCE => (
						SolAddress(bs58_array("4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU")),
						SolAddress(bs58_array("4hD7UM6rQtcqQWtzELvrafpmBYReVXvCpssB6qjY1Sg5")),
					),
					cf_runtime_utilities::genesis_hashes::SISYPHOS => (
						SolAddress(bs58_array("4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU")),
						SolAddress(bs58_array("mYabVW1uMXpGqwgHUBQu4Fg6GT9EMYUzYaGYbi3zgT7")),
					),
					_ => (
						SolAddress(bs58_array("24PNhTaNtomHhoy3fTRaMhAFCRj4uHqhZEEoWrKDbR5p")),
						SolAddress(bs58_array("2tmtGLQcBd11BMiE9B1tAkQXwmPNgR79Meki2Eme4Ec9")),
					),
				}
			);
		}
		Ok(())
	}
}
