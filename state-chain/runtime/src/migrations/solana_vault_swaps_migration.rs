use crate::*;
use chainflip::solana_elections::SolanaVaultSwapsSettings;
use frame_support::{pallet_prelude::Weight, storage::unhashed, traits::UncheckedOnRuntimeUpgrade};

use pallet_cf_elections::{ElectoralSettings, ElectoralUnsynchronisedState};
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

use cf_utilities::bs58_array;
use codec::Encode;

pub struct SolanaVaultSwapsMigration;

impl UncheckedOnRuntimeUpgrade for SolanaVaultSwapsMigration {
	fn on_runtime_upgrade() -> Weight {
		let mut raw_unsynchronised_state = unhashed::get_raw(&ElectoralUnsynchronisedState::<
			Runtime,
			SolanaInstance,
		>::hashed_key())
		.unwrap();
		raw_unsynchronised_state.extend(0u32.encode());
		unhashed::put_raw(
			&ElectoralUnsynchronisedState::<Runtime, SolanaInstance>::hashed_key(),
			&raw_unsynchronised_state[..],
		);

		let (usdc_token_mint_pubkey, swap_endpoint_data_account_address) =
			match cf_runtime_utilities::genesis_hashes::genesis_hash::<Runtime>() {
				cf_runtime_utilities::genesis_hashes::BERGHAIN => (
					SolAddress(bs58_array("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v")),
					SolAddress(bs58_array("FmAcjWaRFUxGWBfGT7G3CzcFeJFsewQ4KPJVG4f6fcob")),
				),
				cf_runtime_utilities::genesis_hashes::PERSEVERANCE => (
					SolAddress(bs58_array("4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU")),
					SolAddress(bs58_array("12MYcNumSQCn81yKRfrk5P5ThM5ivkLiZda979hhKJDR")),
				),
				cf_runtime_utilities::genesis_hashes::SISYPHOS => (
					SolAddress(bs58_array("4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU")),
					SolAddress(bs58_array("EXeku7Q9AiAXBdH7cUHw2ue3okhrofvDZR7EBE1BVQZu")),
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
			unhashed::put_raw(
				&ElectoralSettings::<Runtime, SolanaInstance>::hashed_key_for(key),
				&raw_storage_at_key[..],
			);
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
						SolAddress(bs58_array("FmAcjWaRFUxGWBfGT7G3CzcFeJFsewQ4KPJVG4f6fcob")),
					),
					cf_runtime_utilities::genesis_hashes::PERSEVERANCE => (
						SolAddress(bs58_array("4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU")),
						SolAddress(bs58_array("12MYcNumSQCn81yKRfrk5P5ThM5ivkLiZda979hhKJDR")),
					),
					cf_runtime_utilities::genesis_hashes::SISYPHOS => (
						SolAddress(bs58_array("4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU")),
						SolAddress(bs58_array("EXeku7Q9AiAXBdH7cUHw2ue3okhrofvDZR7EBE1BVQZu")),
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
