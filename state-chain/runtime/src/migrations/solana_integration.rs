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
						vault_program: SolAddress(bs58_array(
							"7ThGuS6a4KmX2rMFhqeCPHrRmmYEF7XoimGG53171xJa",
						)),
						vault_program_data_account: SolAddress(bs58_array(
							"GpTqSHz4JzQimjfDiBgDhJzYcTonj3t6kMhKTigCKHfc",
						)),
						usdc_token_mint_pubkey: SolAddress(bs58_array(
							"4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU",
						)),
						token_vault_pda_account: SolAddress(bs58_array(
							"2Uv7dCnuxuvyFnTRCyEyQpvwyYBhgFkWDm3b5Qdz9Agd",
						)),
						usdc_token_vault_ata: SolAddress(bs58_array(
							"FYQrMSUQx3jrJMpu21mR8qzhpLXfa1nn65ZVqp4QSdEa",
						)),
					},
					Some(SolHash(bs58_array("EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG"))),
					vec![
						(
							const_address("DiNM3dmV4tmJ9sihpXqE6R2MkdyNoArbdU8qfcDHUaRf"),
							const_hash("4DEDrSVk4FRKFQkU1p9Zywi5MK64AGxC16RQZvhyFngq"),
						),
						(
							const_address("65GZq92jgKDX7Bw1DARPZ26JER1Puv9wxo51CE4PWtJo"),
							const_hash("5s1V7bXByHPquC1AYD94w4f8SgEhDjEnGeBtiPsuzXYU"),
						),
						(
							const_address("Yr7ZBvZCnCe2ktQkhjLujvyW8N9nAat2GdoaicJoK3Y"),
							const_hash("7Y1PvrW65rZp3RqmJksix3XxQ9MrFdQ62NNbhdB97qwc"),
						),
						(
							const_address("J35Cfq65BdDz2qH1nqDigJTXhyBik6vApM6AVEy63vmH"),
							const_hash("F1fe16vREumonQurbFAfmbKytfEE9khjy9UPjjgbGnG9"),
						),
						(
							const_address("62hNXX6cW9QSAqSxQEdE6k5c4mQXg8S3h3ZA2CQdFMuJ"),
							const_hash("D6osW2CyHmpLqg8ymDAeNEjZZETqHGWdQBekh3cVjAUQ"),
						),
						(
							const_address("DSKBQs1Zj4QMRt7JPrytJBJKCDmYiWKa5pqnLQQmwADF"),
							const_hash("7qDGqASPR3VannmDosTXUVMd5ZWbqDnawCA3auEHsq6r"),
						),
						(
							const_address("GFUNNyfQVX82yMYYAwhzV5c3eqXegPVt8qTN54TGXwq1"),
							const_hash("4TFDiBqjU5istaaAovdgKBNDKJFdZ318W6XuC9MZiDBC"),
						),
						(
							const_address("ExGFeiZMJf4HBWZZAFfXacY4EnT7TJQrsNsGBaVW1Rtv"),
							const_hash("7ua7UjY1Csouw7K1nMDyWhL7Lx5PE9ernETcKciWALFH"),
						),
						(
							const_address("E2jV7bm8sBNAFDy96Nar5GtsX6n5U18EHM7prUfoDpNt"),
							const_hash("742EN3zJUt6Xcrs1KAH4jfyLLVp8BYV2bSmjEpsFpMFo"),
						),
						(
							const_address("6WcamLU38f1asFanFXYugVJuHN4TXHZicmJgPz9Xr6U7"),
							const_hash("FS1PdTqsDSEa9xUrLAS5k471MQsT28H2FW5CpUHiTmGF"),
						),
					],
				),
				cf_runtime_upgrade_utilities::genesis_hashes::SISYPHOS => (
					SolApiEnvironment {
						vault_program: SolAddress(bs58_array(
							"Gvcsg1ADZJSFXFRp7RUR1Z3DtMZec8iWUPoPVCMv4VQh",
						)),
						vault_program_data_account: SolAddress(bs58_array(
							"DXF45ndZRWkHQvQcFdLuNmT3KHP18VCshJK1mQoLUAWz",
						)),
						usdc_token_mint_pubkey: SolAddress(bs58_array(
							"4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU",
						)),
						token_vault_pda_account: SolAddress(bs58_array(
							"FsQeQkrTWETD8wbZhKyQVfWQLjprjdRG8GAriauXn972",
						)),
						usdc_token_vault_ata: SolAddress(bs58_array(
							"B2d8rCk5jXUfjgYMpVRARQqZ4xh49XNMf7GYUFtdZd6q",
						)),
					},
					Some(SolHash(bs58_array("EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG"))),
					vec![
						(
							const_address("Cr5YnF9p4M91CrQGHJhP3Syy4aGZNVAwF6zvTxkAZZfj"),
							const_hash("9QVwTXtwGTbq4U3KPN9THdnxQ38bVFu6P15cwhURJqNC"),
						),
						(
							const_address("3E14JFszKMCDcxXuGk4mDsBddHxSpXrzZ2ZpGHGr8WJv"),
							const_hash("DkYbyJ5P576ekMQYUuWizejxoWHUUZN3nLrQzVFe2mjd"),
						),
						(
							const_address("C5qNSCcusHvkPrWEt7fQQ8TbgFMkoEetfpigpJEvwam"),
							const_hash("FWUwBbVbRFtaWhpptZ9vsUtiZtc8c6MKKsAnQfRn6uRV"),
						),
						(
							const_address("FG2Akgw76D5GbQZHpmwPNBSMi3pXq4ffZeYrY7sfUCp4"),
							const_hash("3niQmTX5qKD69gdNRLwxRm1o4d65Vkw1QxQH27GLiDCD"),
						),
						(
							const_address("HmqRHTmDbQEhkD3RPR58VM6XtF5Gytod5XmgYz9r5Lyx"),
							const_hash("5ngBYFzxZ2sTFetLY92LiQVzjXZTYbqTjc58ShVZC19d"),
						),
						(
							const_address("FgRZqCYnmjpBY5WA16y73TqRbkLD3zr5btQiSB2B8sr7"),
							const_hash("9C1RDEeKLFT2txok1zvqZ3Fu5K1dxCqDCJc3KPfuBqTn"),
						),
						(
							const_address("BR7Zn41M6enmL5vcfKHnTzr3F5g6rMAG64uDiZYQ5W3Z"),
							const_hash("B8PDaqM9TUjyuKwT8K2C4tiF2p5jTiBX7r1B9gogVen6"),
						),
						(
							const_address("4TdqxPvxST91mbTyup2Pc87MBhVywpt2T7JQP6bAazsp"),
							const_hash("Dne5GaxYgG2KzgpC7aD7XX3pFQp3qvj3vfCFy3kfjaJw"),
						),
						(
							const_address("5c4JZKCroL3Sg6Sm7giqdh57cvatJpSPHpmcJX3uJAMm"),
							const_hash("FKU1qKCydv3TjE1ZDimvevA4khGakkJFRmyVorZvYR7D"),
						),
						(
							const_address("DcEmNXySnth2FNhsHmt64oB15pjtakKhfG3nez7qB52w"),
							const_hash("3iiniRfNTFmBn6Y9ZhovefUDaGaJ7buB2vYemnbFoHN3"),
						),
					],
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
							"9CGLwcPknpYs3atgwtjMX7RhgvBgaqK8wwCvXnmjEoL9",
						)),
					},
					None,
					vec![
						(
							const_address("2cNMwUCF51djw2xAiiU54wz1WrU8uG4Q8Kp8nfEuwghw"),
							const_hash("AYhK82xu8EjSnMdc3PgWuHF8VGM3j2WqK1yKGkCcZZWB"),
						),
						(
							const_address("HVG21SovGzMBJDB9AQNuWb6XYq4dDZ6yUwCbRUuFnYDo"),
							const_hash("8RERC5Lf12ffygzBGaFvvgoYmb7b3GGbVAfsgtD9tY32"),
						),
						(
							const_address("HDYArziNzyuNMrK89igisLrXFe78ti8cvkcxfx4qdU2p"),
							const_hash("mbnF59Y3mKyMCPFVVojYtqqArJ2B9mvKTbCNnP32s9v"),
						),
						(
							const_address("HLPsNyxBqfq2tLE31v6RiViLp2dTXtJRgHgsWgNDRPs2"),
							const_hash("4KhCT1ZZvGR1MT7jGAQsxvMKazUe6rUjPSvMc7FD1zQB"),
						),
						(
							const_address("GKMP63TqzbueWTrFYjRwMNkAyTHpQ54notRbAbMDmePM"),
							const_hash("EyyTVyhpVxox6MGgVV9DQ8WjzF9bfrdfj8ycx8waL77Q"),
						),
						(
							const_address("EpmHm2aSPsB5ZZcDjqDhQ86h1BV32GFCbGSMuC58Y2tn"),
							const_hash("3cuMVDLiomLsQosUjRrReWRdDNfEmokSf91AEfLp21X5"),
						),
						(
							const_address("9yBZNMrLrtspj4M7bEf2X6tqbqHxD2vNETw8qSdvJHMa"),
							const_hash("YC4WTnbVuh4jgYCwWwi7tACYRUFAQeXVg3pcmPsx4D5"),
						),
						(
							const_address("J9dT7asYJFGS68NdgDCYjzU2Wi8uBoBusSHN1Z6JLWna"),
							const_hash("DSXwuxxyTGW7cu7aa2eVapapqLxfp73qWgdoxguc6HkY"),
						),
						(
							const_address("GUMpVpQFNYJvSbyTtUarZVL7UDUgErKzDTSVJhekUX55"),
							const_hash("Hem8T2hC3UUMUudsM8gTDW2g6rcaSxhLs7vpjWf43Gzd"),
						),
						(
							const_address("AUiHYbzH7qLZSkb3u7nAqtvqC7e41sEzgWjBEvXrpfGv"),
							const_hash("HKnmg7JM1F8J2m6sDGdo7Pf5cDkRxaQRbf3fnL3sMsHQ"),
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
