use crate::{chainflip::solana_elections, Runtime};
use cf_chains::{
	instances::SolanaInstance,
	sol::{SolApiEnvironment, SolHash},
};
use cf_utilities::bs58_array;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use sol_prim::consts::{const_address, const_hash};
#[cfg(feature = "try-runtime")]
use sp_runtime::{DispatchError, FixedU128};
use sp_std::vec;

pub struct SolanaIntegration;

impl OnRuntimeUpgrade for SolanaIntegration {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		use cf_chains::sol::SolAddress;

		// Initialize Solana's API environment
		// TODO: PRO-1465 Configure these variables correctly.
		let (sol_env, genesis_hash, durable_nonces_and_accounts) =
			match cf_runtime_upgrade_utilities::genesis_hashes::genesis_hash::<Runtime>() {
				cf_runtime_upgrade_utilities::genesis_hashes::BERGHAIN => (
					SolApiEnvironment {
						vault_program: SolAddress(bs58_array("11111111111111111111111111111111")),
						vault_program_data_account: SolAddress(bs58_array(
							"11111111111111111111111111111111",
						)),
						usdc_token_mint_pubkey: SolAddress(bs58_array(
							"EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
						)),
						token_vault_pda_account: SolAddress(bs58_array(
							"11111111111111111111111111111111",
						)),
						usdc_token_vault_ata: SolAddress(bs58_array(
							"11111111111111111111111111111111",
						)),
					},
					Some(SolHash(bs58_array("5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d"))),
					vec![(
						SolAddress(hex_literal::hex!(
							"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
						)),
						SolHash(hex_literal::hex!(
							"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
						)),
					)],
				),
				cf_runtime_upgrade_utilities::genesis_hashes::PERSEVERANCE => (
					SolApiEnvironment {
						vault_program: SolAddress(bs58_array("11111111111111111111111111111111")),
						vault_program_data_account: SolAddress(bs58_array(
							"11111111111111111111111111111111",
						)),
						usdc_token_mint_pubkey: SolAddress(bs58_array(
							"4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU",
						)),
						token_vault_pda_account: SolAddress(bs58_array(
							"11111111111111111111111111111111",
						)),
						usdc_token_vault_ata: SolAddress(bs58_array(
							"11111111111111111111111111111111",
						)),
					},
					Some(SolHash(bs58_array("EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG"))),
					vec![(
						SolAddress(hex_literal::hex!(
							"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
						)),
						SolHash(hex_literal::hex!(
							"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
						)),
					)],
				),
				cf_runtime_upgrade_utilities::genesis_hashes::SISYPHOS => (
					SolApiEnvironment {
						vault_program: SolAddress(bs58_array("11111111111111111111111111111111")),
						vault_program_data_account: SolAddress(bs58_array(
							"11111111111111111111111111111111",
						)),
						usdc_token_mint_pubkey: SolAddress(bs58_array(
							"4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU",
						)),
						token_vault_pda_account: SolAddress(bs58_array(
							"11111111111111111111111111111111",
						)),
						usdc_token_vault_ata: SolAddress(bs58_array(
							"11111111111111111111111111111111",
						)),
					},
					Some(SolHash(bs58_array("EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG"))),
					vec![(
						SolAddress(hex_literal::hex!(
							"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
						)),
						SolHash(hex_literal::hex!(
							"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
						)),
					)],
				),
				_ => (
					// Assume testnet
					SolApiEnvironment {
						vault_program: SolAddress(bs58_array(
							"8inHGLHXegST3EPLcpisQe9D1hDT9r7DJjS395L3yuYf",
						)),
						vault_program_data_account: SolAddress(bs58_array(
							"wxudAoEJWfe6ZFHYsDPYGGs2K3m62N3yApNxZLGyMYc",
						)),
						usdc_token_mint_pubkey: SolAddress(bs58_array(
							"24PNhTaNtomHhoy3fTRaMhAFCRj4uHqhZEEoWrKDbR5p",
						)),
						token_vault_pda_account: SolAddress(bs58_array(
							"CWxWcNZR1d5MpkvmL3HgvgohztoKyCDumuZvdPyJHK3d",
						)),
						usdc_token_vault_ata: SolAddress(bs58_array(
							"GgqCE4bTwMy4QWVaTRTKJqETAgim49zNrH1dL6zXaTpd",
						)),
					},
					None,
					vec![
						(
							const_address("2cNMwUCF51djw2xAiiU54wz1WrU8uG4Q8Kp8nfEuwghw"),
							const_hash("BcuQnUuMLi7dDGTcW13fPmkf26gZp6cmMycwuzXAPUWq"),
						),
						(
							const_address("HVG21SovGzMBJDB9AQNuWb6XYq4dDZ6yUwCbRUuFnYDo"),
							const_hash("CCFc5QptF1HogqCZHCD1HaEzZRunGJvpqJJEnELsoqrH"),
						),
						(
							const_address("HDYArziNzyuNMrK89igisLrXFe78ti8cvkcxfx4qdU2p"),
							const_hash("CjPEYgDoX2PD5sQKg9pJyBzVFPEy6BYHCZtYpE7LKEPu"),
						),
						(
							const_address("HLPsNyxBqfq2tLE31v6RiViLp2dTXtJRgHgsWgNDRPs2"),
							const_hash("HKx2FNX7rsfzat1Nsr83Xq82LZSBnEB3LyzBmEtFGpAy"),
						),
						(
							const_address("GKMP63TqzbueWTrFYjRwMNkAyTHpQ54notRbAbMDmePM"),
							const_hash("92ijUn6xBxFExuazPhspSSzhZXjQqbXmYre4ztNGWLJ9"),
						),
						(
							const_address("EpmHm2aSPsB5ZZcDjqDhQ86h1BV32GFCbGSMuC58Y2tn"),
							const_hash("BiHQhh7xY1ZFuEuC2vj9rFsZSXvhWA6FcdNDfBsfENbh"),
						),
						(
							const_address("9yBZNMrLrtspj4M7bEf2X6tqbqHxD2vNETw8qSdvJHMa"),
							const_hash("AtxE1WFQGV1hbxmNmAcN7azMRLjaEyp6wk8Y62swxQEz"),
						),
						(
							const_address("J9dT7asYJFGS68NdgDCYjzU2Wi8uBoBusSHN1Z6JLWna"),
							const_hash("Ub9SGgi88Pyrd2ZPRLj6QzsMDf4VCM4U2EBz37f3QWP"),
						),
						(
							const_address("GUMpVpQFNYJvSbyTtUarZVL7UDUgErKzDTSVJhekUX55"),
							const_hash("HgZQKdVeArjJmetoAw4SpYKyzKJh3txaqb7VQe4kJ7tV"),
						),
						(
							const_address("AUiHYbzH7qLZSkb3u7nAqtvqC7e41sEzgWjBEvXrpfGv"),
							const_hash("ZW7wrDEXQe94VrsoG5A6b5iQMEdUPGkc16Vk5rt7pgR"),
						),
					],
				),
			};

		pallet_cf_environment::SolanaApiEnvironment::<Runtime>::put(sol_env);
		pallet_cf_environment::SolanaGenesisHash::<Runtime>::set(genesis_hash);
		pallet_cf_environment::SolanaAvailableNonceAccounts::<Runtime>::set(
			durable_nonces_and_accounts,
		);
		// Ignore errors as it is not dangerous if the pallet fails to initialize (TODO possible
		// makes sense to emit an event though?)
		let _result = pallet_cf_elections::Pallet::<Runtime, SolanaInstance>::internally_initialize(
			(
				/* chain tracking */ Default::default(),
				/* priority_fee */ 100000u32.into(),
				(),
			),
			(
				(),
				solana_elections::SolanaFeeSettings { fee_multiplier: FixedU128::from_u32(1u32) },
				(),
			),
			((), (), ()),
		);
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<sp_std::vec::Vec<u8>, DispatchError> {
		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: sp_std::vec::Vec<u8>) -> Result<(), DispatchError> {
		Ok(())
	}
}
