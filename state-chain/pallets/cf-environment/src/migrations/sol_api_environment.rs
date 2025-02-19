use crate::*;

use frame_support::{pallet_prelude::Weight, traits::UncheckedOnRuntimeUpgrade};

use cf_chains::{
	evm::H256,
	sol::{
		sol_tx_core::consts::{const_address, const_hash},
		AddressLookupTableAccount, SolAddress,
	},
};
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
		pub swap_endpoint_program: SolAddress,
		pub swap_endpoint_program_data_account: SolAddress,
	}
}

pub struct SolApiEnvironmentMigration<T>(PhantomData<T>);

impl<T: Config<Hash = H256>> UncheckedOnRuntimeUpgrade for SolApiEnvironmentMigration<T> {
	fn on_runtime_upgrade() -> Weight {
		log::info!("🌮 Running migration for Environment pallet: Updating SolApiEnvironment.");
		let _ = SolanaApiEnvironment::<T>::translate::<old::SolApiEnvironment, _>(|old_env| {
			old_env.map(
				|old::SolApiEnvironment {
				     vault_program,
				     vault_program_data_account,
				     token_vault_pda_account,
				     usdc_token_mint_pubkey,
				     usdc_token_vault_ata,
				     swap_endpoint_program,
				     swap_endpoint_program_data_account,
				 }| {
					let (alt_manager_program, address_lookup_table_account) =
						match cf_runtime_utilities::genesis_hashes::genesis_hash::<T>() {
							// TODO: To update with real values
							cf_runtime_utilities::genesis_hashes::BERGHAIN => (
								SolAddress(bs58_array(
									"J88B7gmadHzTNGiy54c9Ms8BsEXNdB2fntFyhKpk3qoT",
								)),
								(
									SolAddress(bs58_array(
										"FmAcjWaRFUxGWBfGT7G3CzcFeJFsewQ4KPJVG4f6fcob",
									)),
									vec![],
								),
							),
							// TODO: To update with the right values
							cf_runtime_utilities::genesis_hashes::PERSEVERANCE => (
								SolAddress(bs58_array(
									"DeL6iGV5RWrWh7cPoEa7tRHM8XURAaB4vPjfX5qVyuWE",
								)),
								(
									SolAddress(bs58_array(
										"12MYcNumSQCn81yKRfrk5P5ThM5ivkLiZda979hhKJDR",
									)),
									vec![],
								),
							),
							// TODO: To update with the right values
							cf_runtime_utilities::genesis_hashes::SISYPHOS => (
								SolAddress(bs58_array(
									"FtK6TR2ZqhChxXeDFoVzM9gYDPA18tGrKoBb3hX7nPwt",
								)),
								(
									SolAddress(bs58_array(
										"EXeku7Q9AiAXBdH7cUHw2ue3okhrofvDZR7EBE1BVQZu",
									)),
									vec![],
								),
							),
							_ => (
								SolAddress(bs58_array(
									"49XegQyykAXwzigc6u7gXbaLjhKfNadWMZwFiovzjwUw",
								)),
								(
									SolAddress(bs58_array(
										"7drVSq2ymJLNnXyCciHbNqHyzuSt1SL4iQSEThiESN2c",
									)),
									vec![
										const_address(
											"BttvFNSRKrkHugwDP6SpnBejCKKskHowJif1HGgBtTfG",
										)
										.into(),
										const_address(
											"SysvarRecentB1ockHashes11111111111111111111",
										)
										.into(),
										const_address(
											"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
										)
										.into(),
										const_address(
											"7B13iu7bUbBX88eVBqTZkQqrErnTMazPmGLdE5RqdyKZ",
										)
										.into(),
										const_address(
											"9CGLwcPknpYs3atgwtjMX7RhgvBgaqK8wwCvXnmjEoL9",
										)
										.into(),
										const_address(
											"24PNhTaNtomHhoy3fTRaMhAFCRj4uHqhZEEoWrKDbR5p",
										)
										.into(),
										const_address(
											"Sysvar1nstructions1111111111111111111111111",
										)
										.into(),
										const_address(
											"2tmtGLQcBd11BMiE9B1tAkQXwmPNgR79Meki2Eme4Ec9",
										)
										.into(),
										const_address(
											"EWaGcrFXhf9Zq8yxSdpAa75kZmDXkRxaP17sYiL6UpZN",
										)
										.into(),
										const_address(
											"So11111111111111111111111111111111111111112",
										)
										.into(),
										const_address(
											"ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL",
										)
										.into(),
										const_address("11111111111111111111111111111111").into(),
										const_address(
											"8inHGLHXegST3EPLcpisQe9D1hDT9r7DJjS395L3yuYf",
										)
										.into(),
										const_address(
											"35uYgHdfZQT4kHkaaXQ6ZdCkK5LFrsk43btTLbGCRCNT",
										)
										.into(),
										const_address(
											"49XegQyykAXwzigc6u7gXbaLjhKfNadWMZwFiovzjwUw",
										)
										.into(),
										const_address(
											"2cNMwUCF51djw2xAiiU54wz1WrU8uG4Q8Kp8nfEuwghw",
										)
										.into(),
										const_address(
											"HVG21SovGzMBJDB9AQNuWb6XYq4dDZ6yUwCbRUuFnYDo",
										)
										.into(),
										const_address(
											"HDYArziNzyuNMrK89igisLrXFe78ti8cvkcxfx4qdU2p",
										)
										.into(),
										const_address(
											"HLPsNyxBqfq2tLE31v6RiViLp2dTXtJRgHgsWgNDRPs2",
										)
										.into(),
										const_address(
											"GKMP63TqzbueWTrFYjRwMNkAyTHpQ54notRbAbMDmePM",
										)
										.into(),
										const_address(
											"EpmHm2aSPsB5ZZcDjqDhQ86h1BV32GFCbGSMuC58Y2tn",
										)
										.into(),
										const_address(
											"9yBZNMrLrtspj4M7bEf2X6tqbqHxD2vNETw8qSdvJHMa",
										)
										.into(),
										const_address(
											"J9dT7asYJFGS68NdgDCYjzU2Wi8uBoBusSHN1Z6JLWna",
										)
										.into(),
										const_address(
											"GUMpVpQFNYJvSbyTtUarZVL7UDUgErKzDTSVJhekUX55",
										)
										.into(),
										const_address(
											"AUiHYbzH7qLZSkb3u7nAqtvqC7e41sEzgWjBEvXrpfGv",
										)
										.into(),
										const_address(
											"BN2vyodNYQQTrx3gtaDAL2UGGVtZwFeF5M8krE5aYYES",
										)
										.into(),
										const_address(
											"Gwq9TAQCjbJtdnmtxQa3PbHFfbr6YTUBMDjEP9x2uXnH",
										)
										.into(),
										const_address(
											"3pGbKatko2ckoLEy139McfKiirNgy9brYxieNqFGdN1W",
										)
										.into(),
										const_address(
											"9Mcd8BTievK2yTvyiqG9Ft4HfDFf6mjGFBWMnCSRQP8S",
										)
										.into(),
										const_address(
											"AEZG74RoqM6sxf79eTizq5ShB4JTuCkMVwUgtnC8H94z",
										)
										.into(),
										const_address(
											"APLkgyCWi8DFAMF4KikjTu8YnUG1r7sMjVEfDiaBRZnS",
										)
										.into(),
										const_address(
											"4ShNXTTHvpVt6bQdZTRdyW6yWXDzrPupdMuxajbEoGE4",
										)
										.into(),
										const_address(
											"FgZp6NJYWw15U51ynfXCfU9vq3eVgDDAHMSfJ8fFBZZ8",
										)
										.into(),
										const_address(
											"ENQ9Mmg87KFLX8ncXRPDBSd7jhKCtPBi8QzAh4rkREgP",
										)
										.into(),
										const_address(
											"Hhay1UwkzkFUgrGUYuiCvUwv7kErNzAcZnVRQ2fetT7K",
										)
										.into(),
										const_address(
											"2fUVR42opcHgGLrY1eguDXLYfQPHQe9ReJNmRorVt9v8",
										)
										.into(),
										const_address(
											"HfKr1wJASkW5UHs8yNWAqMeaYJdp8K2mdYwkbdVRdVrm",
										)
										.into(),
										const_address(
											"DrpYkMpJWkpNqX9yYgQfc3uZrCVYobJ3RbTABcSkHJkM",
										)
										.into(),
										const_address(
											"HCXc3o2go1Y2KhfnykLYXEvofLifXTb7GT13w4GsFmGw",
										)
										.into(),
										const_address(
											"FFKYhae4HSnMmA6JJfe8NNtZeySA9yRWLaHzE2jqfhBr",
										)
										.into(),
										const_address(
											"AaRrJovR9Npna4fuCJ17AB3cJAMzoNDaZymRTbGGzUZm",
										)
										.into(),
										const_address(
											"5S8DzBBLvJUeyJccV4DekAK8KJA5PDcjwxRxCvgdyBEi",
										)
										.into(),
										const_address(
											"Cot1DQZpm859brrre7swrDhTYLj2NJbg3hdMKCHk5zSk",
										)
										.into(),
										const_address(
											"4mfDv7PisvtMhiyGmvD6vxRdVpB842XbUhimAZYxMEn9",
										)
										.into(),
										const_address(
											"BHW7qFCNHTX5QD5yJpT1hn1VM817Ji5ksZqiXMfqGrsj",
										)
										.into(),
										const_address(
											"EJqZLeaxi2gVsJgQW4nbmxyWJukK25n7jB8qWKoDgWUN",
										)
										.into(),
										const_address(
											"BJqTPWyoqqgzhkLh1pbPh4KWBqg8kCUNzJ81avitSQrm",
										)
										.into(),
										const_address(
											"EkmPmEmSbwm8EDDYtLtaDgcfuLNtW7MbKx5w3FUpaGjv",
										)
										.into(),
										const_address(
											"CgwtCv8HQ67imnHEkz24TfXfyA2H5jurxcLGxAgDmNQj",
										)
										.into(),
										const_address(
											"zfKsXSxJ4cTpKS7S6aHL1Hy3m1CEjQuySKSwkWvukQX",
										)
										.into(),
										const_address(
											"2VvN1s6txNYyBdKpaC8b6AZKVqUQiQT2Exrpa7ffCgV6",
										)
										.into(),
										const_address(
											"A2DT1dc4rA1uMry7WCLwoUEQQNjCAsAMkB4X9Lgo88zd",
										)
										.into(),
										const_address(
											"9mNBRGfTMLsSsQUn4YZfRDBVXfQ6juEWbNUTwv2ir9gC",
										)
										.into(),
										const_address(
											"3jXiydxPx1P7Ggdja5yt384ryLJAW2c8LRGV8PPRT54C",
										)
										.into(),
										const_address(
											"7ztGR1z28NpYjUaXyrGBzBGu62u1f9H9Pj9UVSKnT3yu",
										)
										.into(),
										const_address(
											"4GdnDTr5X4eJFHuzTEBLrz3tsREo8rQro7S9YDqrbMZ9",
										)
										.into(),
										const_address(
											"ALxnH6TBKJPBFRfFZspQkxDjb9nGLUP5oxFFdZNRFgUu",
										)
										.into(),
										const_address(
											"Bu3sdWtBh5TJishgK3vneh2zJg1rjLqWN5mFTHxWspwJ",
										)
										.into(),
										const_address(
											"GvBbUTE312RXU5iXAcNWt6CuVbfsPs5Nk28D6qvU6NF3",
										)
										.into(),
										const_address(
											"2LLct8SsnkW3sD9Gu8CfxmDEjKAWtFXqLvA8ymMyuq8u",
										)
										.into(),
										const_address(
											"CQ9vUhC3dSa4LyZCpWVpNbXhSn6f7J3NQXWDDvMMk6aW",
										)
										.into(),
										const_address(
											"Cw8GqRmKzCbp7UFfafECC9sf9f936Chgx3BkbSgnXfmU",
										)
										.into(),
										const_address(
											"GFJ6m6YdNT1tUfAxyD2BiPSx8gwt3xe4jVAKdtdSUt8W",
										)
										.into(),
										const_address(
											"7bphTuo5BKs4JJw5WPusCevmnoRk9ocFiB8EGgfwnh4c",
										)
										.into(),
										const_address(
											"EFbUq18Mcdi2gGauRzmbNeD5ixaB7EYVk5JZgAF34LoS",
										)
										.into(),
									],
								),
							),
						};

					cf_chains::sol::SolApiEnvironment {
						vault_program,
						vault_program_data_account,
						token_vault_pda_account,
						usdc_token_mint_pubkey,
						usdc_token_vault_ata,
						swap_endpoint_program,
						swap_endpoint_program_data_account,

						// Newly inserted values
						alt_manager_program,
						address_lookup_table_account: AddressLookupTableAccount {
							key: address_lookup_table_account.0.into(),
							addresses: address_lookup_table_account.1,
						},
					}
				},
			)
		});

		// Insert new nonces into storage
		match cf_runtime_utilities::genesis_hashes::genesis_hash::<T>() {
			cf_runtime_utilities::genesis_hashes::BERGHAIN => (),
			// TODO: To add the right values
			cf_runtime_utilities::genesis_hashes::PERSEVERANCE => (),
			// TODO: To add the right values
			cf_runtime_utilities::genesis_hashes::SISYPHOS => (),
			_ => {
				SolanaAvailableNonceAccounts::<T>::append((
					const_address("BN2vyodNYQQTrx3gtaDAL2UGGVtZwFeF5M8krE5aYYES"),
					const_hash("4fjG6oYKadvnsbzAzomF5k2Zdc4DuuUyT71nueAeykMW"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("Gwq9TAQCjbJtdnmtxQa3PbHFfbr6YTUBMDjEP9x2uXnH"),
					const_hash("GK29hbKjKWNwdF4KT11MzkrmQPsYPwE41qZMnLVcQPaS"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("3pGbKatko2ckoLEy139McfKiirNgy9brYxieNqFGdN1W"),
					const_hash("5cinXdpw2KAGzmiXXegWJRdDDboXbDHaQaT3WFsH3txb"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("9Mcd8BTievK2yTvyiqG9Ft4HfDFf6mjGFBWMnCSRQP8S"),
					const_hash("DRoAyPDtsg9CCMBSN6egFsWsP2zsQBAxCzN6fAdtQxJU"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("AEZG74RoqM6sxf79eTizq5ShB4JTuCkMVwUgtnC8H94z"),
					const_hash("G8ZKHMsWFSoKJAtVbm1xgv8VjT5F6YBeiZbbzpVHuuyM"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("APLkgyCWi8DFAMF4KikjTu8YnUG1r7sMjVEfDiaBRZnS"),
					const_hash("BMUqNXhMoB6VWsR7jHgRcv7yio8L5vjHdGby7gEJ4Pd2"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("4ShNXTTHvpVt6bQdZTRdyW6yWXDzrPupdMuxajbEoGE4"),
					const_hash("52yamKJdMiQ5tEUyhkngvjR3XFXp7dmJzYsVsLbPs9JX"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("FgZp6NJYWw15U51ynfXCfU9vq3eVgDDAHMSfJ8fFBZZ8"),
					const_hash("AX3qKNMBRKZimeCsBEhtp7heeduKekj85a4UpdN34HFe"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("ENQ9Mmg87KFLX8ncXRPDBSd7jhKCtPBi8QzAh4rkREgP"),
					const_hash("GGFme2ydkkbDzq7LhVDMX5SsFf2yGLf7uKNSLLhvrGMd"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("Hhay1UwkzkFUgrGUYuiCvUwv7kErNzAcZnVRQ2fetT7K"),
					const_hash("HMN14axscHALAuknuwSpVkEmAJkZWeJoNAjJuXUjRQbN"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("2fUVR42opcHgGLrY1eguDXLYfQPHQe9ReJNmRorVt9v8"),
					const_hash("RWoH6shzxCS9dmW2mg37cujXxARBRBunbHEtZwUz1Gj"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("HfKr1wJASkW5UHs8yNWAqMeaYJdp8K2mdYwkbdVRdVrm"),
					const_hash("2dhBBWYQE2Fvm4ShUQjno8ydJb5H6rUmBZ1e6TJHDupL"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("DrpYkMpJWkpNqX9yYgQfc3uZrCVYobJ3RbTABcSkHJkM"),
					const_hash("6VCThFyLtFCKk35wihyrRUa6dubBU7skhJdRRDBfH4Md"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("HCXc3o2go1Y2KhfnykLYXEvofLifXTb7GT13w4GsFmGw"),
					const_hash("EggsCqUiKJVxmN7a9s7RScXUAJRjekjwCAaeQvK9TcJ4"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("FFKYhae4HSnMmA6JJfe8NNtZeySA9yRWLaHzE2jqfhBr"),
					const_hash("2E8BayMrqL3on6bhcMms6dsm3PwKcxttFSuHDNe6vZ4B"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("AaRrJovR9Npna4fuCJ17AB3cJAMzoNDaZymRTbGGzUZm"),
					const_hash("D5bhT4vxhrtkfPeyZbvCtwzAnHSwBaa287CZZU8F6fye"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("5S8DzBBLvJUeyJccV4DekAK8KJA5PDcjwxRxCvgdyBEi"),
					const_hash("8o7RkbTeV9r1yMgXaTzjcPys2FEkqieHER6Z5Fdc8hbw"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("Cot1DQZpm859brrre7swrDhTYLj2NJbg3hdMKCHk5zSk"),
					const_hash("Gd86jHRKKSxrho3WKni5HYe6runTFX4cyFUQtcmJeiuk"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("4mfDv7PisvtMhiyGmvD6vxRdVpB842XbUhimAZYxMEn9"),
					const_hash("4YQLN6N7G9nFwT8UVdFE2ZniW1gf89Qjob16wxMThxqN"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("BHW7qFCNHTX5QD5yJpT1hn1VM817Ji5ksZqiXMfqGrsj"),
					const_hash("Ft3vnX4KQBa22CVpPkmvk5QNMGwL2xhQVxQtFJwK5MvJ"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("EJqZLeaxi2gVsJgQW4nbmxyWJukK25n7jB8qWKoDgWUN"),
					const_hash("5tZeUYorHfdh9FYsA9yKjanFRwxjGxM9YLzNAfiwhZUf"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("BJqTPWyoqqgzhkLh1pbPh4KWBqg8kCUNzJ81avitSQrm"),
					const_hash("5ggDcExaPfgjsmrhBS3D2UnRaEPsCGKGDkJqzF7gr92A"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("EkmPmEmSbwm8EDDYtLtaDgcfuLNtW7MbKx5w3FUpaGjv"),
					const_hash("3G7D13ckfLCfDFC3VusXdittLHp6z6dZeUJBHTqcc2iR"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("CgwtCv8HQ67imnHEkz24TfXfyA2H5jurxcLGxAgDmNQj"),
					const_hash("Gikpdfo6SwRqi3nTmQKuCmap3cGwupZd2poiYkUop4Sn"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("zfKsXSxJ4cTpKS7S6aHL1Hy3m1CEjQuySKSwkWvukQX"),
					const_hash("43Kn8Kevfy2cy2fpJEhEZSpmPNwFurL2ERG5FqdSeTsq"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("2VvN1s6txNYyBdKpaC8b6AZKVqUQiQT2Exrpa7ffCgV6"),
					const_hash("FVZKFoZ8WRdsFBp64LpTF1MrH36dHym2XZv7cJ2oYU5"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("A2DT1dc4rA1uMry7WCLwoUEQQNjCAsAMkB4X9Lgo88zd"),
					const_hash("HjtHG8XRKyGiN8VXWmMh9oEviURAv5o2VygKTvsZjAPz"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("9mNBRGfTMLsSsQUn4YZfRDBVXfQ6juEWbNUTwv2ir9gC"),
					const_hash("2S1aHds5wqUSyY4BmAK9YyBNGwzsQSsqGa2iyisF8t6h"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("3jXiydxPx1P7Ggdja5yt384ryLJAW2c8LRGV8PPRT54C"),
					const_hash("Hgu6wkD6aE3uuESdW9fCWoXX4sN3eLnxYJsM7QCtrZMk"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("7ztGR1z28NpYjUaXyrGBzBGu62u1f9H9Pj9UVSKnT3yu"),
					const_hash("9wic99ejEMQubHea9KKZZk27EU7r4LL8672D5GNrpXRG"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("4GdnDTr5X4eJFHuzTEBLrz3tsREo8rQro7S9YDqrbMZ9"),
					const_hash("FCsgGf33ueodTVhLgQtTriNL5ZGfoaWoBkDwXSbDmLFd"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("ALxnH6TBKJPBFRfFZspQkxDjb9nGLUP5oxFFdZNRFgUu"),
					const_hash("QkBgGAKFPtQzRj1v7sFECr8D2LMPb99AEy3w1RtKX5j"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("Bu3sdWtBh5TJishgK3vneh2zJg1rjLqWN5mFTHxWspwJ"),
					const_hash("GWvckQW231Safveswww1GBSu4SzP5h5SgE6gugBn8upC"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("GvBbUTE312RXU5iXAcNWt6CuVbfsPs5Nk28D6qvU6NF3"),
					const_hash("BnFsZdujQde7FnHGVcZTmvidRHBr5H87XRDDB6A5dn8D"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("2LLct8SsnkW3sD9Gu8CfxmDEjKAWtFXqLvA8ymMyuq8u"),
					const_hash("Bnt1CWn8SEwpNqFbNxby6ysoW49wmL95Ed28pbS9v4Nx"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("CQ9vUhC3dSa4LyZCpWVpNbXhSn6f7J3NQXWDDvMMk6aW"),
					const_hash("2yVSXvwXjtA5tqqWUKjxBuYjME6pKwJGA12NUc31x9VS"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("Cw8GqRmKzCbp7UFfafECC9sf9f936Chgx3BkbSgnXfmU"),
					const_hash("FDPW3e2qPvNrmi1dqxsMaYAXLq9vMQYda5fKsVzNBUCv"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("GFJ6m6YdNT1tUfAxyD2BiPSx8gwt3xe4jVAKdtdSUt8W"),
					const_hash("4tUTcUePrDUu48ZyH584aYv8JAbPrc9aDcH6bjxhEJon"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("7bphTuo5BKs4JJw5WPusCevmnoRk9ocFiB8EGgfwnh4c"),
					const_hash("SyhFE8iYH9ZsZNBDWLvTDTBFBoEjxs12msF3xprikgf"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("EFbUq18Mcdi2gGauRzmbNeD5ixaB7EYVk5JZgAF34LoS"),
					const_hash("53EBQYjZ7yH3Zy6KCWjSGAUGTki2YjxsHkrfXnvsj9vT"),
				));
			},
		};

		Weight::zero()
	}
}
