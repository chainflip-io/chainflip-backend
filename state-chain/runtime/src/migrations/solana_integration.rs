use crate::Runtime;
use cf_chains::{
	instances::SolanaInstance,
	sol::{SolApiEnvironment, SolHash, SolTrackedData},
	ChainState,
};
use cf_utilities::bs58_array;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use sol_prim::consts::{const_address, const_hash};
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
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
							"BttvFNSRKrkHugwDP6SpnBejCKKskHowJif1HGgBtTfG",
						)),
						usdc_token_mint_pubkey: SolAddress(bs58_array(
							"24PNhTaNtomHhoy3fTRaMhAFCRj4uHqhZEEoWrKDbR5p",
						)),
						token_vault_pda_account: SolAddress(bs58_array(
							"7B13iu7bUbBX88eVBqTZkQqrErnTMazPmGLdE5RqdyKZ",
						)),
						usdc_token_vault_ata: SolAddress(bs58_array(
							"GgqCE4bTwMy4QWVaTRTKJqETAgim49zNrH1dL6zXaTpd",
						)),
					},
					None,
					vec![
						(
							const_address("2cNMwUCF51djw2xAiiU54wz1WrU8uG4Q8Kp8nfEuwghw"),
							const_hash("GogbV48pmmEaGmipMJc4aLiacEZz9wQHS4CX2zT4PEsm"),
						),
						(
							const_address("HVG21SovGzMBJDB9AQNuWb6XYq4dDZ6yUwCbRUuFnYDo"),
							const_hash("CB6pTV6JXoUewUDXjU49upCkEP4NHE3JQ73nEyYCja44"),
						),
						(
							const_address("HDYArziNzyuNMrK89igisLrXFe78ti8cvkcxfx4qdU2p"),
							const_hash("2u3cnRSQyoUXkjJTy1mLdau14k1AUgB2uFs2JYWoCv71"),
						),
						(
							const_address("HLPsNyxBqfq2tLE31v6RiViLp2dTXtJRgHgsWgNDRPs2"),
							const_hash("94dq3SJWgEQ4Y6gCjQzLcbBeNNRRfUDb5Z1ukcYskNAq"),
						),
						(
							const_address("GKMP63TqzbueWTrFYjRwMNkAyTHpQ54notRbAbMDmePM"),
							const_hash("CUGTVuNuWC2W23UowCaATfVkFzXZ84xhe1hQe2uDs5yw"),
						),
						(
							const_address("EpmHm2aSPsB5ZZcDjqDhQ86h1BV32GFCbGSMuC58Y2tn"),
							const_hash("EhpNvPoHTk59QeDo5RodU1sucPBcZ4ptiw4NgVtuARGp"),
						),
						(
							const_address("9yBZNMrLrtspj4M7bEf2X6tqbqHxD2vNETw8qSdvJHMa"),
							const_hash("8xQKGacN7m2szw8ZFXkjLjGjEEfCFzhp3em1B8sazym9"),
						),
						(
							const_address("J9dT7asYJFGS68NdgDCYjzU2Wi8uBoBusSHN1Z6JLWna"),
							const_hash("4KBbZqjDAeDYDh5ceDJ59fg679YBaw5mR9h2r9pua18S"),
						),
						(
							const_address("GUMpVpQFNYJvSbyTtUarZVL7UDUgErKzDTSVJhekUX55"),
							const_hash("83Kht22e7rReWCRb7Ge2UvY4mwxC83dKfRcDrHCHF5Di"),
						),
						(
							const_address("AUiHYbzH7qLZSkb3u7nAqtvqC7e41sEzgWjBEvXrpfGv"),
							const_hash("6otHj5QS3JaVFo5Vjg52viQp7jdoHccGjxjdcZM5Wmss"),
						),
					],
				),
			};

		pallet_cf_environment::SolanaApiEnvironment::<Runtime>::put(sol_env);
		pallet_cf_environment::SolanaGenesisHash::<Runtime>::set(genesis_hash);
		pallet_cf_environment::SolanaAvailableNonceAccounts::<Runtime>::set(
			durable_nonces_and_accounts,
		);
		pallet_cf_chain_tracking::CurrentChainState::<Runtime, SolanaInstance>::put(ChainState {
			block_height: 0,
			tracked_data: SolTrackedData { priority_fee: 100000u32.into() },
		});
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
