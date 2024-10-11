use frame_support::traits::OnRuntimeUpgrade;

use crate::*;
use frame_support::pallet_prelude::Weight;

use cf_chains::sol::SolAddress;
use cf_utilities::bs58_array;
use codec::{Decode, Encode};
use scale_info::TypeInfo;

pub mod old {
	use super::*;
	use cf_chains::sol::SolAddress;

	#[derive(Encode, Decode, TypeInfo)]
	pub struct SolApiEnvironment {
		pub vault_program: SolAddress,
		pub vault_program_data_account: SolAddress,
		pub token_vault_pda_account: SolAddress,
		pub usdc_token_mint_pubkey: SolAddress,
		pub usdc_token_vault_ata: SolAddress,
	}
}

pub struct SolApiEnvironmentMigration;

impl OnRuntimeUpgrade for SolApiEnvironmentMigration {
	fn on_runtime_upgrade() -> Weight {
		log::info!("🌮 Running migration for Environment pallet: Updating SolApiEnvironment.");
		let _ = pallet_cf_environment::SolanaApiEnvironment::<Runtime>::translate::<
			old::SolApiEnvironment,
			_,
		>(|old_env| {
			old_env.map(
				|old::SolApiEnvironment {
				     vault_program,
				     vault_program_data_account,
				     token_vault_pda_account,
				     usdc_token_mint_pubkey,
				     usdc_token_vault_ata,
				 }| {
					let (swap_endpoint_program, swap_endpoint_program_data_account) =
						match cf_runtime_upgrade_utilities::genesis_hashes::genesis_hash::<Runtime>(
						) {
							cf_runtime_upgrade_utilities::genesis_hashes::BERGHAIN => (
								SolAddress(bs58_array(
									"BnECXbsDFYPmhxcV57dodaWtJJjtGPE8Le3LAR7qieYb",
								)),
								SolAddress(bs58_array(
									"5mFsKrqCH5v9Q9uF5o6qrsUi1GV2myuhc23NAi5YFs4M",
								)),
							),
							cf_runtime_upgrade_utilities::genesis_hashes::PERSEVERANCE => (
								SolAddress(bs58_array(
									"FXN1iLmQ47c962nackmzBWZxXE8BR9AXy8mu34oFdKiy",
								)),
								SolAddress(bs58_array(
									"4hD7UM6rQtcqQWtzELvrafpmBYReVXvCpssB6qjY1Sg5",
								)),
							),
							cf_runtime_upgrade_utilities::genesis_hashes::SISYPHOS => (
								SolAddress(bs58_array(
									"7G6TxoGDsgaZX3HaKkrKyy28tsdr7ZGmeeMbXpm8R5HZ",
								)),
								SolAddress(bs58_array(
									"mYabVW1uMXpGqwgHUBQu4Fg6GT9EMYUzYaGYbi3zgT7",
								)),
							),
							_ => (
								SolAddress(bs58_array(
									"35uYgHdfZQT4kHkaaXQ6ZdCkK5LFrsk43btTLbGCRCNT",
								)),
								SolAddress(bs58_array(
									"2tmtGLQcBd11BMiE9B1tAkQXwmPNgR79Meki2Eme4Ec9",
								)),
							),
						};

					cf_chains::sol::SolApiEnvironment {
						vault_program,
						vault_program_data_account,
						token_vault_pda_account,
						usdc_token_mint_pubkey,
						usdc_token_vault_ata,

						// Newly inserted values
						swap_endpoint_program,
						swap_endpoint_program_data_account,
					}
				},
			)
		});

		Weight::zero()
	}
}
