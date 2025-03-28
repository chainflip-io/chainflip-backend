// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

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
							cf_runtime_utilities::genesis_hashes::SISYPHOS => (
								SolAddress(bs58_array(
									"6mDRToYmsEzuTmEZ5SdNcd2y4UDVEZ4xJSFvk4FjnvXG",
								)),
								(
									SolAddress(bs58_array(
										"Ast7ygd4AMPuy6ZUsk4FnDKCUkdcVR2T9ZQT8aAxveGu",
									)),
									vec![
										const_address(
											"DXF45ndZRWkHQvQcFdLuNmT3KHP18VCshJK1mQoLUAWz",
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
											"FsQeQkrTWETD8wbZhKyQVfWQLjprjdRG8GAriauXn972",
										)
										.into(),
										const_address(
											"B2d8rCk5jXUfjgYMpVRARQqZ4xh49XNMf7GYUFtdZd6q",
										)
										.into(),
										const_address(
											"4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU",
										)
										.into(),
										const_address(
											"Sysvar1nstructions1111111111111111111111111",
										)
										.into(),
										const_address(
											"EXeku7Q9AiAXBdH7cUHw2ue3okhrofvDZR7EBE1BVQZu",
										)
										.into(),
										const_address(
											"APzLHyWY4CZtTjk5ynxCLW2E2W9R1DY4yFeGNhwSeBzg",
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
											"Gvcsg1ADZJSFXFRp7RUR1Z3DtMZec8iWUPoPVCMv4VQh",
										)
										.into(),
										const_address(
											"FtK6TR2ZqhChxXeDFoVzM9gYDPA18tGrKoBb3hX7nPwt",
										)
										.into(),
										const_address(
											"6mDRToYmsEzuTmEZ5SdNcd2y4UDVEZ4xJSFvk4FjnvXG",
										)
										.into(),
										const_address(
											"Cr5YnF9p4M91CrQGHJhP3Syy4aGZNVAwF6zvTxkAZZfj",
										)
										.into(),
										const_address(
											"3E14JFszKMCDcxXuGk4mDsBddHxSpXrzZ2ZpGHGr8WJv",
										)
										.into(),
										const_address(
											"C5qNSCcusHvkPrWEt7fQQ8TbgFMkoEetfpigpJEvwam",
										)
										.into(),
										const_address(
											"FG2Akgw76D5GbQZHpmwPNBSMi3pXq4ffZeYrY7sfUCp4",
										)
										.into(),
										const_address(
											"HmqRHTmDbQEhkD3RPR58VM6XtF5Gytod5XmgYz9r5Lyx",
										)
										.into(),
										const_address(
											"FgRZqCYnmjpBY5WA16y73TqRbkLD3zr5btQiSB2B8sr7",
										)
										.into(),
										const_address(
											"BR7Zn41M6enmL5vcfKHnTzr3F5g6rMAG64uDiZYQ5W3Z",
										)
										.into(),
										const_address(
											"4TdqxPvxST91mbTyup2Pc87MBhVywpt2T7JQP6bAazsp",
										)
										.into(),
										const_address(
											"5c4JZKCroL3Sg6Sm7giqdh57cvatJpSPHpmcJX3uJAMm",
										)
										.into(),
										const_address(
											"DcEmNXySnth2FNhsHmt64oB15pjtakKhfG3nez7qB52w",
										)
										.into(),
										const_address(
											"5xj8KDCVGLPvXMjeUjvnekTZUh1ojyfJUaYdmP4augDj",
										)
										.into(),
										const_address(
											"pdyvBtXeDVxGWDs6kJjLtQf8PmLZbhUTyoz2ogohavu",
										)
										.into(),
										const_address(
											"34jPE4S9PupsyqcQ13av6wg7MzsntRvNsq72woipCXG5",
										)
										.into(),
										const_address(
											"FNVLBq9FMfsUBTbJjULMENtg6sLtNb74LvjaXZfFanRF",
										)
										.into(),
										const_address(
											"BBadHPGbJJSAWZgYgXdTZGrYYT6SrqRVJyg9JUGrQbnx",
										)
										.into(),
										const_address(
											"Fp38wuVb1sn8usCtN6nzb54R2MwRwVcmV2z63yjabBxP",
										)
										.into(),
										const_address(
											"3KddDN5PFoMcty9wSfUUZTpfQJadurq1dETPsV3dMf6m",
										)
										.into(),
										const_address(
											"2fFrkZYHM9ZxkTHqA9yPitVVaxNkTr2Vb93TQdNwfAAG",
										)
										.into(),
										const_address(
											"2H8vKQTSdMe296LTKaQpuXg2jZ9wgwcgUXxL2CY9As7w",
										)
										.into(),
										const_address(
											"FMxKBbsXdgwroxDedrokE3yPJLUg3xMnze8widSgbvaZ",
										)
										.into(),
										const_address(
											"GMeTF6WqDAjGBGqLsebVbckZGcvsxbGHEWvLupGrpgZp",
										)
										.into(),
										const_address(
											"2vUVkEPWY2Ckw9Cwtd1WU3htJS6UUQCLoVtzkSey9U5G",
										)
										.into(),
										const_address(
											"7SKjU5Pdnc5Ux5BAFMN1hEqcVseM71jEGsDqqUgWktv2",
										)
										.into(),
										const_address(
											"4ZcUnRpJitLd4yTm9vLd3obCzHG5EYTGZprjkatugXRJ",
										)
										.into(),
										const_address(
											"AhkHTwnDGZjz7kqmAEEGiEUyDKijHsENt3KjzgfiLT6K",
										)
										.into(),
										const_address(
											"4ABNV5jDexAKxrnUy9XVFyvCtnvSK7M8k1kZRqhdWABf",
										)
										.into(),
										const_address(
											"9H87SQJn25aVnB8YrnrCZHNwy18AKow1SsBEFM5ubYbh",
										)
										.into(),
										const_address(
											"9cmsCRypzNeZ8tEPqSM92jRvjdET1m6J2JkJv9YsNmV5",
										)
										.into(),
										const_address(
											"Du4QkRu2rVwLcFBUJAGQ2DXPHTz6mVfNLNVyid5o6Vm6",
										)
										.into(),
										const_address(
											"AZHLvwNcGdZP1AsGHFc2hzs11APJiGbEhkyK5VpyCBKa",
										)
										.into(),
										const_address(
											"7hVJaSegGTdVtDwZ9iNJyPuSD3HX3iZ9SDdCsqShkypc",
										)
										.into(),
										const_address(
											"8NwHCwPfzpyQvQxXTypmw4QQdHxLpZrmuyJ2wBRny2cE",
										)
										.into(),
										const_address(
											"FQyP8Pe4xFaeu1wPEaA3nqor3UrtWdFMYTXq4J92JEoZ",
										)
										.into(),
										const_address(
											"3B3Vwvfx1ZWwcrf1i5F26w4zs7SpMva4JZMnMob8FKvs",
										)
										.into(),
										const_address(
											"FRB7dgrjcvvGc4faqhXQyzPwvNBacx7AQoGURiA721q9",
										)
										.into(),
										const_address(
											"6jGyYPcu1QRfyV7s99QZ5DyaTzsjbvaDcyTNiYfF2j2k",
										)
										.into(),
										const_address(
											"CcGQ73N19U5Po99FrcjLsCHLsSdvT276tCmesZckxzrc",
										)
										.into(),
										const_address(
											"7zne7jv6cvTLBaTTMCFvvqXwpMdqwwSdWY58n2v7xXgY",
										)
										.into(),
										const_address(
											"FfRe1ZrayiNd4uVrCg8CoWKHvZrdQZqGpSHT9BPMLq5N",
										)
										.into(),
										const_address(
											"8xqgHheNm75KgfxXrwTH84vVCJFbRfgiDYikaXLcpEgv",
										)
										.into(),
										const_address(
											"5DrhcUmXwoWLwzeCU3xVhAjg1MHL8JqcpAisX645NPSW",
										)
										.into(),
										const_address(
											"98ENa65H4azGmaEdn3kx7VMmy5Hx73jZdShAbvQaaTy5",
										)
										.into(),
										const_address(
											"B1LUePw4D7PwcFqNbbNBSYJjopgBQSV4NYKmEgqNAN5v",
										)
										.into(),
										const_address(
											"AdKGe6Bv1qFUUzoLv9BQKRn49RCM7sVxrHVy5zniAznn",
										)
										.into(),
										const_address(
											"BQPXeAXL89DcffrdfCpqNNcu5ehQdvHYZL75pLS1GMxg",
										)
										.into(),
										const_address(
											"G5xssHyVV1r3bLRastAXnr27cvB3KYBMjwEDD5H4nqxU",
										)
										.into(),
										const_address(
											"Gj5CfJA4nP6m5xHekk28QRAanJUJEFFx2fjKHUdSagGY",
										)
										.into(),
										const_address(
											"G9dvMwe1hJuSGrnqLdkbSnuWH386iL3UuxYuJz64FeLf",
										)
										.into(),
										const_address(
											"BuCN3zHPSfy1489ajoiVD3cNstpLMrePyeTs4QAcENyH",
										)
										.into(),
										const_address(
											"2zMqwgU9xm4foAaHGnYKiWANePwb4bhfYREyU9HSK6Eb",
										)
										.into(),
									],
								),
							),
							_ => (
								SolAddress(bs58_array(
									"49XegQyykAXwzigc6u7gXbaLjhKfNadWMZwFiovzjwUw",
								)),
								(
									SolAddress(bs58_array(
										"DevMVEbBZirFWmiVu851LUY3d6ajRassAKghUhrHvNSb",
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
			// TODO: To add the right values
			cf_runtime_utilities::genesis_hashes::BERGHAIN => (),
			// TODO: To add the right values
			cf_runtime_utilities::genesis_hashes::PERSEVERANCE => (),
			cf_runtime_utilities::genesis_hashes::SISYPHOS => {
				SolanaAvailableNonceAccounts::<T>::append((
					const_address("5xj8KDCVGLPvXMjeUjvnekTZUh1ojyfJUaYdmP4augDj"),
					const_hash("6HzAWG8d1AQonZ3pwLWJV9WrWYgwxnUJY2GhgttJKkcA"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("pdyvBtXeDVxGWDs6kJjLtQf8PmLZbhUTyoz2ogohavu"),
					const_hash("4hrU2kCAk6a74dZisvVr1ZWkJhPVjQAVCL9mA8NzWu7z"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("34jPE4S9PupsyqcQ13av6wg7MzsntRvNsq72woipCXG5"),
					const_hash("8HQRmyAmGkBQUEyBDLhb8jPjy1uixksUz91CcFc6JyK8"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("FNVLBq9FMfsUBTbJjULMENtg6sLtNb74LvjaXZfFanRF"),
					const_hash("7MziAmsVQKffKjQ3RoJXnJqJ2F49bGQndhHaAHDXoi8K"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("BBadHPGbJJSAWZgYgXdTZGrYYT6SrqRVJyg9JUGrQbnx"),
					const_hash("41No3gjHRq5ZYX6EgvACfg2pxathmBg1T4sz7pfLY8v1"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("Fp38wuVb1sn8usCtN6nzb54R2MwRwVcmV2z63yjabBxP"),
					const_hash("9pgAxFfYWugjraiLoP5NzeMhdmAbB4EQkwKmcfjxG6cG"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("3KddDN5PFoMcty9wSfUUZTpfQJadurq1dETPsV3dMf6m"),
					const_hash("47jUSZ7yLSfB17gQHVBZM3bwYxFL8VvWGaTweoXqZtZC"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("2fFrkZYHM9ZxkTHqA9yPitVVaxNkTr2Vb93TQdNwfAAG"),
					const_hash("8vkXMgZ16FNjZb68VdtQwp1uzWCnpZJnY91uA8syXNdT"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("2H8vKQTSdMe296LTKaQpuXg2jZ9wgwcgUXxL2CY9As7w"),
					const_hash("4UEB4R2oCiaLbMk125KxcRamoPNPHaGWLwBTM8UQGwhC"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("FMxKBbsXdgwroxDedrokE3yPJLUg3xMnze8widSgbvaZ"),
					const_hash("BkjLb5HguMDpYVPtFUcNaZyFD1v46HiLu4Bf4MNiEvWb"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("GMeTF6WqDAjGBGqLsebVbckZGcvsxbGHEWvLupGrpgZp"),
					const_hash("GyXPxjydxXVrkqN3ETXwSHnzDj4un6wFCorEwGEkR7SU"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("2vUVkEPWY2Ckw9Cwtd1WU3htJS6UUQCLoVtzkSey9U5G"),
					const_hash("EYVUL9Ev6Mp57vZLEDsHZF1VCrWQGQTs1fHKCN2jAxQx"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("7SKjU5Pdnc5Ux5BAFMN1hEqcVseM71jEGsDqqUgWktv2"),
					const_hash("9g7vZTi7GSjc5WqCUhKXWq3gjGLSy2p4Y3gyVzQizbER"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("4ZcUnRpJitLd4yTm9vLd3obCzHG5EYTGZprjkatugXRJ"),
					const_hash("3avMm96VgjwGhMDWHx5kLXgokJELQrRUJgCXEMXrNSdZ"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("AhkHTwnDGZjz7kqmAEEGiEUyDKijHsENt3KjzgfiLT6K"),
					const_hash("BVz34q9yTapvLNVDap3kNHwKJUiCZhxWLFHmWdc7BrZ9"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("4ABNV5jDexAKxrnUy9XVFyvCtnvSK7M8k1kZRqhdWABf"),
					const_hash("Fc2ni2WuyG9HMN5uAgd2224GVUTmXYoUsBr8nNMLdViB"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("9H87SQJn25aVnB8YrnrCZHNwy18AKow1SsBEFM5ubYbh"),
					const_hash("En9FXjhbusvfM8PGwYL8AWeoLizD23fZNTW2QPfLoFKg"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("9cmsCRypzNeZ8tEPqSM92jRvjdET1m6J2JkJv9YsNmV5"),
					const_hash("BBpzrANo5dMgucF4YiKW9e2hTyVvLp8LY4tKDjiDpToB"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("Du4QkRu2rVwLcFBUJAGQ2DXPHTz6mVfNLNVyid5o6Vm6"),
					const_hash("DJExDrHJgU7hMH2TQ4AEgpSwhQJnEEe7eZgDChPbnBs5"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("AZHLvwNcGdZP1AsGHFc2hzs11APJiGbEhkyK5VpyCBKa"),
					const_hash("9f3DfcMBLu3dGy13BkL4ppji5C9ehd3eFkSmMMwMSrTn"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("7hVJaSegGTdVtDwZ9iNJyPuSD3HX3iZ9SDdCsqShkypc"),
					const_hash("EEF1GCZd7Fz3E3n29CsKpeFUpFkeKAgSUgAfN4VFCufv"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("8NwHCwPfzpyQvQxXTypmw4QQdHxLpZrmuyJ2wBRny2cE"),
					const_hash("3D9qkRrHqShAq52Gh9mhd6EaeKNBi5jAJNtYpzhzz4xd"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("FQyP8Pe4xFaeu1wPEaA3nqor3UrtWdFMYTXq4J92JEoZ"),
					const_hash("4yrAPq83REY4mDBdM1t6fRfwVHch9TcX17cvCTLyERYC"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("3B3Vwvfx1ZWwcrf1i5F26w4zs7SpMva4JZMnMob8FKvs"),
					const_hash("8oEKxLGFXU9LzXyoDN3B1tfoWSp6VywxKu8xsK2HRkTS"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("FRB7dgrjcvvGc4faqhXQyzPwvNBacx7AQoGURiA721q9"),
					const_hash("71JSbJaBQ1HTkexV4kjKAtpLsQ7Pu9hXfwW7jTX4yFbi"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("6jGyYPcu1QRfyV7s99QZ5DyaTzsjbvaDcyTNiYfF2j2k"),
					const_hash("HsvC3gJdTqV2u65eh8wCLMHuvyr5KeTrkFoa6D2g1SSf"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("CcGQ73N19U5Po99FrcjLsCHLsSdvT276tCmesZckxzrc"),
					const_hash("CK4KzudMWypcGmSpYVQrsLowfrxVSaMYhu2sYMmUrBf8"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("7zne7jv6cvTLBaTTMCFvvqXwpMdqwwSdWY58n2v7xXgY"),
					const_hash("8ZBZThmtPS38udebpzec1ZhzV1jdgTov7A3cKEnhioMJ"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("FfRe1ZrayiNd4uVrCg8CoWKHvZrdQZqGpSHT9BPMLq5N"),
					const_hash("3z7TyNB45CGHvqiggGsY2zGAB97XgwnptjQNJJLsV6kU"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("8xqgHheNm75KgfxXrwTH84vVCJFbRfgiDYikaXLcpEgv"),
					const_hash("HqDEAy8ThkARjvccr74x8XQ68aT7R4RYsPLagBWU5Xmy"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("5DrhcUmXwoWLwzeCU3xVhAjg1MHL8JqcpAisX645NPSW"),
					const_hash("AEadjgMmchmwY23hG1sj9qkmVTRywCp3FsszrdDYUJaP"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("98ENa65H4azGmaEdn3kx7VMmy5Hx73jZdShAbvQaaTy5"),
					const_hash("EyMjSfSdvR8dy7NGVm88GWo4sm2RA7qAVup4FFYLQf2c"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("B1LUePw4D7PwcFqNbbNBSYJjopgBQSV4NYKmEgqNAN5v"),
					const_hash("GWDnjcW7VEyzFh7eDyfLvSUHu8iR7g1ins7URV7qcpMw"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("AdKGe6Bv1qFUUzoLv9BQKRn49RCM7sVxrHVy5zniAznn"),
					const_hash("4C24gnmBZVXgSxxtvbuZA2RqJkoYZtP39hTbwajJJDy2"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("BQPXeAXL89DcffrdfCpqNNcu5ehQdvHYZL75pLS1GMxg"),
					const_hash("333YxW6k2ib1dCaDW1rZkBCyoZNpFaXWidvPWEm5suG4"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("G5xssHyVV1r3bLRastAXnr27cvB3KYBMjwEDD5H4nqxU"),
					const_hash("6fy7NNJYt6tiEJKB177E1pVBrTUhksvQoGEdFGbrm6Rd"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("Gj5CfJA4nP6m5xHekk28QRAanJUJEFFx2fjKHUdSagGY"),
					const_hash("4vYWgeUrwDb7PfXoJS9pBLbtXRAug9C6AxwhQeLBb3ta"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("G9dvMwe1hJuSGrnqLdkbSnuWH386iL3UuxYuJz64FeLf"),
					const_hash("8wnnU5x6agyiJgKARZmmf2TRYjobxJGLoQz19LaEzV1A"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("BuCN3zHPSfy1489ajoiVD3cNstpLMrePyeTs4QAcENyH"),
					const_hash("TNugQtRn4NaC8kFJZaq7zi97mZgC96mCag1j9JBcQdr"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("2zMqwgU9xm4foAaHGnYKiWANePwb4bhfYREyU9HSK6Eb"),
					const_hash("NhPfjnNeYwsKT2YwruVVGTNWaJSdgMsxnEwHGZ6cwW2"),
				));
			},
			_ => {
				SolanaAvailableNonceAccounts::<T>::append((
					const_address("BN2vyodNYQQTrx3gtaDAL2UGGVtZwFeF5M8krE5aYYES"),
					const_hash("GrZ21MGdPNfVGpMbC7yFiqNStoRjYi4Hw4pmiqcBnaaj"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("Gwq9TAQCjbJtdnmtxQa3PbHFfbr6YTUBMDjEP9x2uXnH"),
					const_hash("3L7PSsX58vXtbZoWoCHpmKfuWGBWgPH7duSPnYW7BKTP"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("3pGbKatko2ckoLEy139McfKiirNgy9brYxieNqFGdN1W"),
					const_hash("F7JuJ8RKYWGNfwf63Y9m6GBQFNzpMfMBnPrVT89dQzfV"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("9Mcd8BTievK2yTvyiqG9Ft4HfDFf6mjGFBWMnCSRQP8S"),
					const_hash("FZmSB3pDqzE4KdNd8EmBPPpqN8FKgB88DNKXs1L1CmgK"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("AEZG74RoqM6sxf79eTizq5ShB4JTuCkMVwUgtnC8H94z"),
					const_hash("D6w3Q65KGGCSVLYBXk8HeyJPd3Wfi7ywqKuQA6WD95Eh"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("APLkgyCWi8DFAMF4KikjTu8YnUG1r7sMjVEfDiaBRZnS"),
					const_hash("Fte11ZNRR5tZieLiK7TVmCzWdqfyTktkpjQBo65ji6Rm"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("4ShNXTTHvpVt6bQdZTRdyW6yWXDzrPupdMuxajbEoGE4"),
					const_hash("4i8DRRYVMXhAy517pwvTTda9VS6AsD1DVK55rd4rhmSF"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("FgZp6NJYWw15U51ynfXCfU9vq3eVgDDAHMSfJ8fFBZZ8"),
					const_hash("BdrBRAQeUym5R7KKFtVZHBLdu5csb9N4bfTj6q9cvPvo"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("ENQ9Mmg87KFLX8ncXRPDBSd7jhKCtPBi8QzAh4rkREgP"),
					const_hash("79boPVjqDj49oeM9gekFpvzHi3NbPkqaboJLRW1ebp8S"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("Hhay1UwkzkFUgrGUYuiCvUwv7kErNzAcZnVRQ2fetT7K"),
					const_hash("2j3V4yEsLQBFkHAFpYVJE2zSBcn4MZGctdkGYycY7cJr"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("2fUVR42opcHgGLrY1eguDXLYfQPHQe9ReJNmRorVt9v8"),
					const_hash("BrcGnjB8iwSo61YDr23Udg5exZ2rrQyUWnjQBdiXgm6Q"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("HfKr1wJASkW5UHs8yNWAqMeaYJdp8K2mdYwkbdVRdVrm"),
					const_hash("ARfKJp7fjXwM3TEPiYbYSwB7MXTCn72mWcaJD5YD4JEb"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("DrpYkMpJWkpNqX9yYgQfc3uZrCVYobJ3RbTABcSkHJkM"),
					const_hash("8ocFizTc8y47pSiXFVApLZ7A1sNc8qChj6h8XmAvr36D"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("HCXc3o2go1Y2KhfnykLYXEvofLifXTb7GT13w4GsFmGw"),
					const_hash("Brrg6v64nU2qEDRV6mUQYmL8oZjJC7sw8MnkeniAv2Un"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("FFKYhae4HSnMmA6JJfe8NNtZeySA9yRWLaHzE2jqfhBr"),
					const_hash("4W7BYj7BzZCudnkrUESAcn3SNshwXDNGPWnW1qdLKZRK"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("AaRrJovR9Npna4fuCJ17AB3cJAMzoNDaZymRTbGGzUZm"),
					const_hash("H8ozgM2tnY2BrtgUHWtnLDNAsNqtFinx2M1rufFyC8GW"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("5S8DzBBLvJUeyJccV4DekAK8KJA5PDcjwxRxCvgdyBEi"),
					const_hash("HUPysNeqUKTgoS4vJ6AVaiKwpxsLprJD5jmcA7yFkhjd"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("Cot1DQZpm859brrre7swrDhTYLj2NJbg3hdMKCHk5zSk"),
					const_hash("JBbeFz5NWAZDyaf7baRVWfxHRNzfTt6uLVycabrdqyFr"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("4mfDv7PisvtMhiyGmvD6vxRdVpB842XbUhimAZYxMEn9"),
					const_hash("8NsEEoAQZ1jfnwPVubwm3jx3LnwUdBiWgvSqTzkypGwX"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("BHW7qFCNHTX5QD5yJpT1hn1VM817Ji5ksZqiXMfqGrsj"),
					const_hash("BU8A5DWHf9imu2FACGcDLvmoFNj6YjQZNVhkGurLHEGq"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("EJqZLeaxi2gVsJgQW4nbmxyWJukK25n7jB8qWKoDgWUN"),
					const_hash("55fo5L9j5YarVYautVVuaLnfUTbkoQwhJK22skVTqsaM"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("BJqTPWyoqqgzhkLh1pbPh4KWBqg8kCUNzJ81avitSQrm"),
					const_hash("BviTbyREbcX8ENNj3iW143JGTZLF37F2jtRWSbWqvpoc"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("EkmPmEmSbwm8EDDYtLtaDgcfuLNtW7MbKx5w3FUpaGjv"),
					const_hash("Bw6PNsg3AgaNkrwmCRVVt4FQ1qMvTLtacvzM4WcHJ2Gn"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("CgwtCv8HQ67imnHEkz24TfXfyA2H5jurxcLGxAgDmNQj"),
					const_hash("GCQi8coVrWpiYDg7kr7XFgHgjWjAR1983Q54pKQ373Ak"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("zfKsXSxJ4cTpKS7S6aHL1Hy3m1CEjQuySKSwkWvukQX"),
					const_hash("9gESB9ApcxXBKE7Z2qx9gxLC3oXYyjMzE4qTCVhkbtiC"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("2VvN1s6txNYyBdKpaC8b6AZKVqUQiQT2Exrpa7ffCgV6"),
					const_hash("J6wsTZ1wUb8XPfiqoZkJp58mat2keh3qh2BrWSTHUrC"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("A2DT1dc4rA1uMry7WCLwoUEQQNjCAsAMkB4X9Lgo88zd"),
					const_hash("93ScfMZZCwMqxJAKEc2PRYvBroDoVywFmmhZoiSRp6kb"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("9mNBRGfTMLsSsQUn4YZfRDBVXfQ6juEWbNUTwv2ir9gC"),
					const_hash("wbHfqsNRVmATYbvtjeJ2GZzWXK8CiUS9wCawuwXUWSQ"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("3jXiydxPx1P7Ggdja5yt384ryLJAW2c8LRGV8PPRT54C"),
					const_hash("J4ijyFp2VeSyVpaxdfaFQsVjAuEeXTzYybzA9KAfpzpZ"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("7ztGR1z28NpYjUaXyrGBzBGu62u1f9H9Pj9UVSKnT3yu"),
					const_hash("2rBreiwLCTH8sbBuCcttgPpGkjwvtVYujTHQj9urqqgA"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("4GdnDTr5X4eJFHuzTEBLrz3tsREo8rQro7S9YDqrbMZ9"),
					const_hash("3Kpkfz28P7vyGeJTxt15UcsfkqWHBa6DcdtxfFAAxjgf"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("ALxnH6TBKJPBFRfFZspQkxDjb9nGLUP5oxFFdZNRFgUu"),
					const_hash("9Qb2PWxkZUV8SXWckWxrmyXq7ykAHz9WMEiCdFBiu9LF"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("Bu3sdWtBh5TJishgK3vneh2zJg1rjLqWN5mFTHxWspwJ"),
					const_hash("DJSiZtVdcY82pHUknCEGGWutz82tApuhact8wmPvogvV"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("GvBbUTE312RXU5iXAcNWt6CuVbfsPs5Nk28D6qvU6NF3"),
					const_hash("5twVG69gCWidRsicKncB6AuDQssunLukFFW3mWe5xjEt"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("2LLct8SsnkW3sD9Gu8CfxmDEjKAWtFXqLvA8ymMyuq8u"),
					const_hash("FzsrqQ6XjjXfUZ7zsrg2n4QpWHPUinh158KkRjJkqfgS"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("CQ9vUhC3dSa4LyZCpWVpNbXhSn6f7J3NQXWDDvMMk6aW"),
					const_hash("EqNgQDEUDnmg7mkHQYxkD6Pp3VeDsF6ppWkyk2jKN7K9"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("Cw8GqRmKzCbp7UFfafECC9sf9f936Chgx3BkbSgnXfmU"),
					const_hash("B6bodiG9vDL6zfzoY7gaWKBeRD7RyuZ8mSbK4fU9rguy"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("GFJ6m6YdNT1tUfAxyD2BiPSx8gwt3xe4jVAKdtdSUt8W"),
					const_hash("Bm37GpK9n83QK9cUaZ6Zrc8TGvSxK2EfJuYCPQEZ2WKb"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("7bphTuo5BKs4JJw5WPusCevmnoRk9ocFiB8EGgfwnh4c"),
					const_hash("3r7idtLjppis2HtbwcttUES6h7GejNnBVA1ueB6ijBWE"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("EFbUq18Mcdi2gGauRzmbNeD5ixaB7EYVk5JZgAF34LoS"),
					const_hash("4b9CDrda1ngSV86zkDVpAwUy64uCdqNYMpK4MQpxwGWT"),
				));
			},
		};

		Weight::zero()
	}
}
