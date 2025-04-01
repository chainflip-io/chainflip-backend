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
							cf_runtime_utilities::genesis_hashes::BERGHAIN => (
								SolAddress(bs58_array(
									"9WuKqnyB4i7sFh2VkrPSW4dkJ3pB8wePQ9MeHEUudsvV",
								)),
								(
									SolAddress(bs58_array(
										"CFdyxyubce52nvCVVP3XEgCyJmxMVg5yf4VV41aybh9k",
									)),
									vec![
										const_address(
											"ACLMuTFvDAb3oecQQGkTVqpUbhCKHG3EZ9uNXHK1W9ka",
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
											"4ZhKJgotJ2tmpYs9Y2NkgJzS7Ac5sghrU4a6cyTLEe7U",
										)
										.into(),
										const_address(
											"8KNqCBB1LKWbtjNxY9v2g1fSBKm2ZRgNNv7rmx2bE6Ce",
										)
										.into(),
										const_address(
											"EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
										)
										.into(),
										const_address(
											"Sysvar1nstructions1111111111111111111111111",
										)
										.into(),
										const_address(
											"FmAcjWaRFUxGWBfGT7G3CzcFeJFsewQ4KPJVG4f6fcob",
										)
										.into(),
										const_address(
											"3tJ67qa2GDfvv2wcMYNUfN5QBZrFpTwcU8ASZKMvCTVU",
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
											"AusZPVXPoUM8QJJ2SL4KwvRGCQ22cDg6Y4rg7EvFrxi7",
										)
										.into(),
										const_address(
											"J88B7gmadHzTNGiy54c9Ms8BsEXNdB2fntFyhKpk3qoT",
										)
										.into(),
										const_address(
											"9WuKqnyB4i7sFh2VkrPSW4dkJ3pB8wePQ9MeHEUudsvV",
										)
										.into(),
										const_address(
											"BDKywh4jrvMEFRUkX1bzK8JoyXBY7cmjaZh7bRFpMX4o",
										)
										.into(),
										const_address(
											"2sZp8mnaNZW5FLpbys4rG7RCpVWixmWydRyJmPzNgxi4",
										)
										.into(),
										const_address(
											"J4Afw1uLrnsQQwEUHQiPe71H3Y3gJQ1oZer5q1QBMViC",
										)
										.into(),
										const_address(
											"CCqwvHKHuUSRxxbV7RLSnSYt7XaFrQtFEmaVumLVNmJK",
										)
										.into(),
										const_address(
											"3bVqyf58hQHsxbjnqnSkopnoyEHB9v9KQwhZj7h1DucW",
										)
										.into(),
										const_address(
											"5iKkv5RTvHKzn4VdYLWu48dYsPz5tVniUEa3wHrG9hjB",
										)
										.into(),
										const_address(
											"3GGKqshYCGcnQKp6iNh8kb5nbZwtNKSbA9Y7H11eAgyU",
										)
										.into(),
										const_address(
											"A2mR1Ytk7R8kGvnRxVLTurZzGr9FwvD8A2ovt3ZRCwQS",
										)
										.into(),
										const_address(
											"HS6RiBAt9FbC62xJ6kLAH4ekpCW8ZE7HiuHZKNaUbk7a",
										)
										.into(),
										const_address(
											"14AwUr3FG75E66aaLy7jCbVGaxGCGLdqtpVyBNAFwKac",
										)
										.into(),
										const_address(
											"Gvn18oy3EZydPGmowvSZYVQSA3Tt1tpAF1D7sdS4oqmc",
										)
										.into(),
										const_address(
											"EGq7QuR2tjwEWTT2dqBgbH911cSzCQrosw5fawhv1Ynb",
										)
										.into(),
										const_address(
											"Hh4pQvB4CvyVsDf3HY2CPW7RXNihUKnnWdY6Q3Et7XC7",
										)
										.into(),
										const_address(
											"75iFAcFXJL2NUoAGdK4x2L3J6HzKGkphiQV4t7WBWRkG",
										)
										.into(),
										const_address(
											"F8Sje34REeRP7GenCkA6aWAaZr23MDd37sZnP95fnWUZ",
										)
										.into(),
										const_address(
											"441a8gL3LWFunH4FzmSvPdnXdSAf5uAk9G83UM3trSLs",
										)
										.into(),
										const_address(
											"9q3kbdHjAUvFtM7scz6FFoUZpQsPD6Ynv47hPcpaLHcg",
										)
										.into(),
										const_address(
											"DfhR7xCzGsFiEW35dNEEa28MiRx8ST2AENYLKRf7DQs1",
										)
										.into(),
										const_address(
											"DviGUYND6o5LsmBAcuVJDb3R5ZNwNs4PCieDCQ7W1oxG",
										)
										.into(),
										const_address(
											"FDVTwERCAq2X6GDczSmf8mg3Hk8G2Wsjupw75QjpryfM",
										)
										.into(),
										const_address(
											"5ibkrb5M6Kd2MYTfobY5oeLPp8QXKTnKJLKhqsVKXfWF",
										)
										.into(),
										const_address(
											"2dLkhYpCtmyXufU5msW3THSjmGGUTnAoziT3ukBTKQbu",
										)
										.into(),
										const_address(
											"Fzh6VuMyoQDj9it51joLf1zDV6GAzjqXE7aXUN7DDkDi",
										)
										.into(),
										const_address(
											"HnH4nSudUL7JAjtLrPxPX5adRFR7HJTtCkrA5d5NvLP3",
										)
										.into(),
										const_address(
											"7TpvwcE7tXuUkrYJ1JbVsq6r1bH3ea2i9nmEuZWXgBfp",
										)
										.into(),
										const_address(
											"FEzs883Xc3j6pk9e4Daox9Yd78h3Xxx9u9ub2GbdK4Fi",
										)
										.into(),
										const_address(
											"Cr9fHbshkweGk6z4DqVg1FG8dNoWfrgYrsJShpY4L15V",
										)
										.into(),
										const_address(
											"HdYL4KEULHN2NKunFPChcGFUYgvqpnZSkLUZUbta1ADj",
										)
										.into(),
										const_address(
											"82vjKymhRCzSTyULmYnoxXkB5WdH69WT3tVX5gPbap77",
										)
										.into(),
										const_address(
											"7kCR5ExCF9r9Tk84aqvXWvBWGv1r3X96W5XmiBbGE1yA",
										)
										.into(),
										const_address(
											"6oiAkK8o3KL3w8pbMJpv99jnuuNi57BdB6HqThjgkpB7",
										)
										.into(),
										const_address(
											"3rPs5e1risNQUY8Bh1HVUwFzs89qCGTGY7nvby12PHxt",
										)
										.into(),
										const_address(
											"GknzUHNX8qF3AsHY1dvdDMa2tN3Fua6kCkpKpLzvC7pc",
										)
										.into(),
										const_address(
											"6qMTRduNAJeofRhyxPNYK5LgaYgLsgBve3r7aH2Jg5BZ",
										)
										.into(),
										const_address(
											"9korXEj8h9o4GNwju9DTk8HCXKCQfafR5jB1a5gykjcF",
										)
										.into(),
										const_address(
											"EXvDJycst46TtzW4p2J6xzMJcAGoNDYno5fiCuxoNAMd",
										)
										.into(),
										const_address(
											"5w7Efx8GR1fJ1cKPgTsK7rzeu1T2VqfQXPNgtPfsKfW8",
										)
										.into(),
										const_address(
											"G4ntoKddtNrBu9TiAqBGmrboY54fCDDKg2S8M822xuwS",
										)
										.into(),
										const_address(
											"DW2en8bGK7C9yxtBaEniLbCaYm5hETG79LEsLK4cXCxs",
										)
										.into(),
										const_address(
											"4hXQXaS4nyGLS2LBV5Z4iWyAs61LP4DCTkr66oUV8qqk",
										)
										.into(),
										const_address(
											"3AprWhiw1EGF1SsayNbCQDj7DV5BZ9naRSBB2LS6GzAD",
										)
										.into(),
										const_address(
											"BL6h9KaC1RJZuAD7mK1TtwJXQJ5YQC3zcD9xEvWMguQ4",
										)
										.into(),
										const_address(
											"e8t1kzNkwvU7fczonysbGU6HkKuQSHKfDHWhACuH9VL",
										)
										.into(),
										const_address(
											"G29wnM1eQrHqZeV43GvrEf5RsK2VbjecnS94S7bCGSP3",
										)
										.into(),
										const_address(
											"DefhiKiD54tGa1LTpwM4LevYtUenGUi36UkBkQqAfvet",
										)
										.into(),
										const_address(
											"69HunmA67d16zAK14biGGAZMiMqn9qBJCfxcRAjJMm3E",
										)
										.into(),
										const_address(
											"8WawSHgiexniQ52H7KcxqaxQpknCYythW6ghPCfWfj62",
										)
										.into(),
										const_address(
											"GgtqYVQdaUj1k6WSnyYfEHUMy7Nf8jxze2jy8UenuNwE",
										)
										.into(),
										const_address(
											"6LihAbvnT46GXKfjqQaPPiB7nadusCrFyyY1Pe5A6m3W",
										)
										.into(),
										const_address(
											"CeTTyF33ZDijNs9MCCMZVtoUFi5WBFVmZVrEkhoWhiHc",
										)
										.into(),
									],
								),
							),
							cf_runtime_utilities::genesis_hashes::PERSEVERANCE => (
								SolAddress(bs58_array(
									"GFyWuzUsmLZF9nd5JdkwFGz91mTpH2p7ctwWV4xL262k",
								)),
								(
									SolAddress(bs58_array(
										"BnpXYuUEDuKnTCi7Dmyco7YehKgjr3HG5gKUMZCLPYcd",
									)),
									vec![
										const_address(
											"GpTqSHz4JzQimjfDiBgDhJzYcTonj3t6kMhKTigCKHfc",
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
											"2Uv7dCnuxuvyFnTRCyEyQpvwyYBhgFkWDm3b5Qdz9Agd",
										)
										.into(),
										const_address(
											"FYQrMSUQx3jrJMpu21mR8qzhpLXfa1nn65ZVqp4QSdEa",
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
											"12MYcNumSQCn81yKRfrk5P5ThM5ivkLiZda979hhKJDR",
										)
										.into(),
										const_address(
											"2BcYzxGN9CeSNo4dF61533xS3ytgwJxRyFYMoNSoZjUp",
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
											"7ThGuS6a4KmX2rMFhqeCPHrRmmYEF7XoimGG53171xJa",
										)
										.into(),
										const_address(
											"DeL6iGV5RWrWh7cPoEa7tRHM8XURAaB4vPjfX5qVyuWE",
										)
										.into(),
										const_address(
											"GFyWuzUsmLZF9nd5JdkwFGz91mTpH2p7ctwWV4xL262k",
										)
										.into(),
										const_address(
											"DiNM3dmV4tmJ9sihpXqE6R2MkdyNoArbdU8qfcDHUaRf",
										)
										.into(),
										const_address(
											"65GZq92jgKDX7Bw1DARPZ26JER1Puv9wxo51CE4PWtJo",
										)
										.into(),
										const_address(
											"Yr7ZBvZCnCe2ktQkhjLujvyW8N9nAat2GdoaicJoK3Y",
										)
										.into(),
										const_address(
											"J35Cfq65BdDz2qH1nqDigJTXhyBik6vApM6AVEy63vmH",
										)
										.into(),
										const_address(
											"62hNXX6cW9QSAqSxQEdE6k5c4mQXg8S3h3ZA2CQdFMuJ",
										)
										.into(),
										const_address(
											"DSKBQs1Zj4QMRt7JPrytJBJKCDmYiWKa5pqnLQQmwADF",
										)
										.into(),
										const_address(
											"GFUNNyfQVX82yMYYAwhzV5c3eqXegPVt8qTN54TGXwq1",
										)
										.into(),
										const_address(
											"ExGFeiZMJf4HBWZZAFfXacY4EnT7TJQrsNsGBaVW1Rtv",
										)
										.into(),
										const_address(
											"E2jV7bm8sBNAFDy96Nar5GtsX6n5U18EHM7prUfoDpNt",
										)
										.into(),
										const_address(
											"6WcamLU38f1asFanFXYugVJuHN4TXHZicmJgPz9Xr6U7",
										)
										.into(),
										const_address(
											"CMLQg4VYFDaqe5qvNxAUCTE9NnNj8otKXzCevMtxfjLj",
										)
										.into(),
										const_address(
											"Gk3gW2MyQD6snCtxWzgKs2XWwChANwv4M6EpyTFkWup5",
										)
										.into(),
										const_address(
											"6ayvGJQENCzWLTzPEzj53Y7mmehNRMXNi2k8vz3gAqtf",
										)
										.into(),
										const_address(
											"2CCSu6BUaMvBQg7FY5X8Xnydav3ZRav8ZBn5FH9tw7JN",
										)
										.into(),
										const_address(
											"7o88CvWSyN1DXA3h6ToLaBiLGV5t1bUjfYDw7taqWm1v",
										)
										.into(),
										const_address(
											"Evr4zeJiov3oa4BSHqMTxrZUt9XfuT6av2uJvAM2Mqip",
										)
										.into(),
										const_address(
											"6sr2XvWBuWbGEDKPAsgkBkbzf3Fkw4Dz2gsxyvTC7pVto",
										)
										.into(),
										const_address(
											"4eS5VXGhCRzuNTTNYM8HEL8E7bQsdNKtK18cXLV23UZr",
										)
										.into(),
										const_address(
											"6wBjG8FiX1QyNXYYCyZ4Zx34QvcUBiChED2G1hB33N2T",
										)
										.into(),
										const_address(
											"3ajoR3xVnL9inT7UiiazPjm9iutqoXsU4wSkh66fZPfd",
										)
										.into(),
										const_address(
											"FCcH2APZJxt2A22URAfQnoQK7PdFa475xrFWq6mB7ZfG",
										)
										.into(),
										const_address(
											"5kds7Pr8cQ34DNArJ1J5Z27hZzU9BHDuh7488rfnZXbF",
										)
										.into(),
										const_address(
											"4TELTJALNTeSefjfFqBoVn8HQsw6G8CYSE82J8DE8tTL",
										)
										.into(),
										const_address(
											"Aijb1yGWDC6Cm1xY4nC1ME1cyRykxUMtngturzKhjBgp",
										)
										.into(),
										const_address(
											"BcZmicHXu5wZbvPPwbUDkHoixcwkPakqSWN8nHtMr5oe",
										)
										.into(),
										const_address(
											"8zHumyn2MUpRkfAaCQt4TpxxeAp9aouxamRKWFjzWSuF",
										)
										.into(),
										const_address(
											"Hu3XtnRDiDZjvc6p8C8H1NgJRvVePLjhEX8CpJPPPJEj",
										)
										.into(),
										const_address(
											"DR7kLFdAr3pcK6h8aaXAjDcbGEs3kChuCavT11NJt7P5",
										)
										.into(),
										const_address(
											"7BgXAEgipjP9AThCCJ7SFVQdsDk3AUJfVhPFFuSLUR4x",
										)
										.into(),
										const_address(
											"4G35PHXZdYSRyT7A3EJfwQVgQRCts9TgKxXjmxsge3hd",
										)
										.into(),
										const_address(
											"3xNwTebfQMHcMWBjMUQgC28LcPWsVkxUdo2BPhoZKfhN",
										)
										.into(),
										const_address(
											"CiB2wJA9mRFNEXpTbshom9444uq36DaYwBdFD6mNchr1",
										)
										.into(),
										const_address(
											"BMLnePDZyjZ1vFuMeuupyf7fk8khwDBqnrWxkVEVRpK8",
										)
										.into(),
										const_address(
											"4xXYUs1gLQtJfYYVeACLT1cyp7nXUAFCyh1viwBnpDeC",
										)
										.into(),
										const_address(
											"GtcdEqFE5oRyLhaNdvuhGeW9eTJ4Daa4Hv2WWmxDDVr1",
										)
										.into(),
										const_address(
											"ERWBS6yjvHxTJznmgcvrLErC6rFNSLmevFy84a3TLBd",
										)
										.into(),
										const_address(
											"8HtyqYYXhQWDkanGK5wcmqQHDwCLyJmZo5928kVQ2GrS",
										)
										.into(),
										const_address(
											"4agjA9Fu11dSX7m99EXiVA3KZTkgfB5EasL2ffjcBcct",
										)
										.into(),
										const_address(
											"DX4BQoMGzqRu3HMheJhbHWL7jq5tiP8w8C4Wz17byX6H",
										)
										.into(),
										const_address(
											"74nJ8cohZk1PH89kCBxpZP6dMfccs4f2UWdwK4q2frqx",
										)
										.into(),
										const_address(
											"CoXNoXnaeHq8X8N1tzh3Av1ykqiWH6ZqFXVL5ta2ktWm",
										)
										.into(),
										const_address(
											"Awbe9xyu4qDpZeKSQeGJSJpgJQAk1iUzL86ZQSMCHDo7",
										)
										.into(),
										const_address(
											"7z7pPPNaHkQj6XyeNbwxnzn4uCmSYgdXSzx5XwcYSVKY",
										)
										.into(),
										const_address(
											"HzqQwC6LJXMxz2sYqdxKBp9NvySZ9uJJYXLAX4KPSims",
										)
										.into(),
										const_address(
											"4EtSnoG2nVsp3o5exSm1rvWtJwCZfZTbCpnB4MaEKemM",
										)
										.into(),
										const_address(
											"7LpBjm9SxR8aweyx2xkWN4RKEp4KKG7yCLS7rpftjiJP",
										)
										.into(),
										const_address(
											"95b6pSdAH92MBfKzgCq9gSUY8sHh1XujgBzhjkFm1Qtu",
										)
										.into(),
										const_address(
											"B9dg1C8nf8YfKjshoo3mbWRwNV4Q3PLekzYTvoAcy2qr",
										)
										.into(),
										const_address(
											"ARigks6etiwnis7ch75EUoyLvMgx5CSVFv72kSv5baoA",
										)
										.into(),
										const_address(
											"HtnVN4WsDx1LfSxy1i4v8jcFZjoyWmUas6zAESF17UAZ",
										)
										.into(),
									],
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
			cf_runtime_utilities::genesis_hashes::BERGHAIN => {
				SolanaAvailableNonceAccounts::<T>::append((
					const_address("Gvn18oy3EZydPGmowvSZYVQSA3Tt1tpAF1D7sdS4oqmc"),
					const_hash("6UEw8EEzB4ttJgq3kJvFn1iLTk18KvSJ8Md1vxiEkZqt"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("EGq7QuR2tjwEWTT2dqBgbH911cSzCQrosw5fawhv1Ynb"),
					const_hash("6S18X45DpiABYgSxCgVTukUokf3Q3ZemkD8NF1gFfCmz"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("Hh4pQvB4CvyVsDf3HY2CPW7RXNihUKnnWdY6Q3Et7XC7"),
					const_hash("Dp6yaqEvMbsZuiL3E8jdyTqwSpwir5B42twjxLThR8oi"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("75iFAcFXJL2NUoAGdK4x2L3J6HzKGkphiQV4t7WBWRkG"),
					const_hash("5xqqeUMSqpmQ1M5W2itgc47HQGoQLeV3uKZGoVECtUBX"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("F8Sje34REeRP7GenCkA6aWAaZr23MDd37sZnP95fnWUZ"),
					const_hash("Cs35kXNUMCMgoqfdcTyKuRfqGVTbEGVDAwPZidsYgYyn"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("441a8gL3LWFunH4FzmSvPdnXdSAf5uAk9G83UM3trSLs"),
					const_hash("DFoiT9qfUjMhxp4UgeTmfDj3iHTW8sGAPjKuPzusc6Vn"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("9q3kbdHjAUvFtM7scz6FFoUZpQsPD6Ynv47hPcpaLHcg"),
					const_hash("DJ7BJaMyEyG8LUczagLPCdtQ6yGkv6wam1Gxqy1f99Y3"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("DfhR7xCzGsFiEW35dNEEa28MiRx8ST2AENYLKRf7DQs1"),
					const_hash("AtVm1fT2qgxZTSdbj2st6u3AvbW25mzSvbpKt8YiZFDw"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("DviGUYND6o5LsmBAcuVJDb3R5ZNwNs4PCieDCQ7W1oxG"),
					const_hash("JAPsKsCv2FquBzgVxw7hbHw7xJqm99zANCZFXrvd83GL"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("FDVTwERCAq2X6GDczSmf8mg3Hk8G2Wsjupw75QjpryfM"),
					const_hash("5sHmsBrbBQqhJyZJRdDr7Mgf27XegeLCf9W1YwCKB1n2"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("5ibkrb5M6Kd2MYTfobY5oeLPp8QXKTnKJLKhqsVKXfWF"),
					const_hash("4DAJ9Gt1DXoMUyuEbeWT4k5ms672TRWi5rLchoj9KRty"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("2dLkhYpCtmyXufU5msW3THSjmGGUTnAoziT3ukBTKQbu"),
					const_hash("2bs5XUHdNQvH4zuDY19WasgmnpDKoHdRQ7LQk1W4Gv2M"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("Fzh6VuMyoQDj9it51joLf1zDV6GAzjqXE7aXUN7DDkDi"),
					const_hash("CAVfhDy4[/vWjZ7TLAs92PTKRE5cywQDyKnex3XCD3pcYf"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("HnH4nSudUL7JAjtLrPxPX5adRFR7HJTtCkrA5d5NvLP3"),
					const_hash("8QTuQLRST5bcpjy8yhqDsuDUA1m82VKW51wKqpUkbohF"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("7TpvwcE7tXuUkrYJ1JbVsq6r1bH3ea2i9nmEuZWXgBfp"),
					const_hash("8xMcfW95iis3QmwRed29XDS5fPZSkRXGTJkkdLj43ZA3"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("FEzs883Xc3j6pk9e4Daox9Yd78h3Xxx9u9ub2GbdK4Fi"),
					const_hash("H5mMuFNGbBmTnxRA1ngFqvqHFSStBfqNwwSF29LmRiaz"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("Cr9fHbshkweGk6z4DqVg1FG8dNoWfrgYrsJShpY4L15V"),
					const_hash("71ypT5pKT5q6qfYFLjUBiYsAWKGYuyKka5X8db3oz26q"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("HdYL4KEULHN2NKunFPChcGFUYgvqpnZSkLUZUbta1ADj"),
					const_hash("FZoG4f465iN96rVgGZ5rLEf9SqS3cVUifzMMk9enZ2Zr"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("82vjKymhRCzSTyULmYnoxXkB5WdH69WT3tVX5gPbap77"),
					const_hash("8ZbEvSSWyi2DM3BEU8wYna17S3hp8N7vJbKPhzhurh6c"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("7kCR5ExCF9r9Tk84aqvXWvBWGv1r3X96W5XmiBbGE1yA"),
					const_hash("2MWDZKzHjiugxprfnRLrVVc9nvEEMnu1k4EggPSUYNLn"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("6oiAkK8o3KL3w8pbMJpv99jnuuNi57BdB6HqThjgkpB7"),
					const_hash("B5jYinLvxL8nELQtA746Nw73EyD4Fa6Hp7sDYQSQDssC"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("3rPs5e1risNQUY8Bh1HVUwFzs89qCGTGY7nvby12PHxt"),
					const_hash("9ep23qh2YBYNR33WK1vJhGVCVER2CjjWa8kTYQdVfH4B"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("GknzUHNX8qF3AsHY1dvdDMa2tN3Fua6kCkpKpLzvC7pc"),
					const_hash("Civh5UjRPVn5Gz2QGh9fmHQRBSoQB5PB7rsYW62xbUhw"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("6qMTRduNAJeofRhyxPNYK5LgaYgLsgBve3r7aH2Jg5BZ"),
					const_hash("8WXexE58i5kdTeDJmsRzdWc3dHtRSZYnwxiF21m8HTnG"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("9korXEj8h9o4GNwju9DTk8HCXKCQfafR5jB1a5gykjcF"),
					const_hash("HAN7Tn2CoKPpogEKDPfW5CCFEDqAdEUGWpfe55s5HPKu"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("EXvDJycst46TtzW4p2J6xzMJcAGoNDYno5fiCuxoNAMd"),
					const_hash("8w2vHaXYibhTW7Y9AFJhLEmJS3xR4wvkASJ8CGUpuZ49"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("5w7Efx8GR1fJ1cKPgTsK7rzeu1T2VqfQXPNgtPfsKfW8"),
					const_hash("ZV8Z1dgDHrPJocsqy9kPdHRMxHNfLzTBdEUNmCAJktP"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("G4ntoKddtNrBu9TiAqBGmrboY54fCDDKg2S8M822xuwS"),
					const_hash("64oEUGLNVeEg8S8Y3NBUesNGUbps75tiChpQzb4E5rrQ"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("DW2en8bGK7C9yxtBaEniLbCaYm5hETG79LEsLK4cXCxs"),
					const_hash("8JaGEpABTpsMehyUB4Rtsfjxf6oWJFTR6RKkMQexrKA1"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("4hXQXaS4nyGLS2LBV5Z4iWyAs61LP4DCTkr66oUV8qqk"),
					const_hash("BVgTfnLxkedAouCu9aaadReDHsd4ibQu4tjPVxK73Y2n"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("3AprWhiw1EGF1SsayNbCQDj7DV5BZ9naRSBB2LS6GzAD"),
					const_hash("2Bnj8PdTbGpgwQCGB3PySoXaS57gYhVWt3XoStYV1HBP"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("BL6h9KaC1RJZuAD7mK1TtwJXQJ5YQC3zcD9xEvWMguQ4"),
					const_hash("e6pGcrkMnhBY1aLS1EzCJrSKtJED2dps2PPCYUCapB5"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("e8t1kzNkwvU7fczonysbGU6HkKuQSHKfDHWhACuH9VL"),
					const_hash("A5outvSBwTVRL4wkwTxLGj2JM7nUv68afmvL44Dq6PEp"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("G29wnM1eQrHqZeV43GvrEf5RsK2VbjecnS94S7bCGSP3"),
					const_hash("9B43eXxdApVRT4u129u3Qxp749H777JLtCeR5UTzncch"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("DefhiKiD54tGa1LTpwM4LevYtUenGUi36UkBkQqAfvet"),
					const_hash("Brg5UziLTWU2ouxchyURSX53MTbRbYjoAFTCR2inyVr8"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("69HunmA67d16zAK14biGGAZMiMqn9qBJCfxcRAjJMm3E"),
					const_hash("71UJG6yxZzV9QppATPReRqM7oNY1UbxJtaWgZa2koY3h"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("8WawSHgiexniQ52H7KcxqaxQpknCYythW6ghPCfWfj62"),
					const_hash("3csxLEwWzMjWKNfHy1Rraym7QXNfpGunmgYNJckYc89n"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("GgtqYVQdaUj1k6WSnyYfEHUMy7Nf8jxze2jy8UenuNwE"),
					const_hash("Goh4K5Y4TVbhyThuQRyAe1Ghh8gjMn9oujzejgupmz3w"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("6LihAbvnT46GXKfjqQaPPiB7nadusCrFyyY1Pe5A6m3W"),
					const_hash("BvsLHsz8GnGdnpY6r8PZwp3w3FUrb6kpP5V2yNU2Jgcd"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("CeTTyF33ZDijNs9MCCMZVtoUFi5WBFVmZVrEkhoWhiHc"),
					const_hash("BRVa4YUgwLrXnJDiuRtBt8yihWA4PPchEMpMGopCPRtr"),
				));
			},
			cf_runtime_utilities::genesis_hashes::PERSEVERANCE => {
				SolanaAvailableNonceAccounts::<T>::append((
					const_address("CMLQg4VYFDaqe5qvNxAUCTE9NnNj8otKXzCevMtxfjLj"),
					const_hash("HxpJVtvo3EttTNQM6sESBscGoaJR58enZ9cuTTbqUzcd"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("Gk3gW2MyQD6snCtxWzgKs2XWwChANwv4M6EpyTFkWup5"),
					const_hash("9CyzXe3NCsUgB9k3r2HCRFMhenkyacwSykrmQ29CcDYj"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("6ayvGJQENCzWLTzPEzj53Y7mmehNRMXNi2k8vz3gAqtf"),
					const_hash("DhNWoCZrJuonGxJqDwdyCzfbAGMbNNz5JwPWHVMroGMq"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("2CCSu6BUaMvBQg7FY5X8Xnydav3ZRav8ZBn5FH9tw7JN"),
					const_hash("39TLUY4Y4ZyqrEW1u9j7rvT9QWaRQZGrXdL7SZqpJssh"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("7o88CvWSyN1DXA3h6ToLaBiLGV5t1bUjfYDw7taqWm1v"),
					const_hash("9QRV9yeAQLRTsPc1zMDRX9rck99x5H4sWAVZt3zhXGKc"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("Evr4zeJiov3oa4BSHqMTxrZUt9XfuT6av2uJvAM2Mqip"),
					const_hash("HYHzrshGKChERtYgfRghjsrUMHCVudE9tj2vAgPw9EKZ"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("6sr2XvWBuWbUdKPAsgkBkbzf3Fkw4Dz2gsxyvTC7pVto"),
					const_hash("9Ra2pUxov3HW6RVoBbkpfTFNeeT4t2xBRyd6hLtPFLLj"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("4eS5VXGhCRzuNTTNYM8HEL8E7bQsdNKtK18cXLV23UZr"),
					const_hash("fC5DeZUoKaHYH7VyTVp52NnugGxWLqQMoGPkHDYtCkq"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("6wBjG8FiX1QyNXYYCyZ4Zx34QvcUBiChED2G1hB33N2T"),
					const_hash("ALvLn2hz6fVBzWP2TeNUtoPWxL7HSW1ZtNA3Hr5tjZFp"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("3ajoR3xVnL9inT7UiiazPjm9iutqoXsU4wSkh66fZPfd"),
					const_hash("2Gm55NmvSH9iJWy9bxkWHZd9qPeLUj6dmPsoSKSrEorT"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("FCcH2APZJxt2A22URAfQnoQK7PdFa475xrFWq6mB7ZfG"),
					const_hash("9XhNSC6NMv7c933YVBHWwEXDtjsbapY69PDTsgYWKKXA"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("5kds7Pr8cQ34DNArJ1J5Z27hZzU9BHDuh7488rfnZXbF"),
					const_hash("Fobahr56M4NB6YVMJQcEv4cFqooyznuBcPUosAgmnNJ"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("4TELTJALNTeSefjfFqBoVn8HQsw6G8CYSE82J8DE8tTL"),
					const_hash("trvVhwb1WrMrFqUU1gbXMznwsvDjgbzJrrUJrJ3hK2Y"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("Aijb1yGWDC6Cm1xY4nC1ME1cyRykxUMtngturzKhjBgp"),
					const_hash("54TJnzWvuiG1bqLqYDjbDtSKe9K1xajKL9B8u8zpeie3"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("BcZmicHXu5wZbvPPwbUDkHoixcwkPakqSWN8nHtMr5oe"),
					const_hash("9fLWLzMikscXYbdaPHvFbqMHsGZd5emcS7ZnfXXRQKfe"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("8zHumyn2MUpRkfAaCQt4TpxxeAp9aouxamRKWFjzWSuF"),
					const_hash("7ZqQsP4FXdHtjrndUreN9mS53nojYF74CWy6A7dr4ZPw"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("Hu3XtnRDiDZjvc6p8C8H1NgJRvVePLjhEX8CpJPPPJEj"),
					const_hash("5XUk8GyAfJwJVg3YawC5MrfAu7itvm97nhWdXSSinqrY"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("DR7kLFdAr3pcK6h8aaXAjDcbGEs3kChuCavT11NJt7P5"),
					const_hash("EjRFkUykbycwsb5mEyh8XDhouBXg5DfDuSCeF7X1a58t"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("7BgXAEgipjP9AThCCJ7SFVQdsDk3AUJfVhPFFuSLUR4x"),
					const_hash("2RH7wS52Ug49xqkdKPMiYKVNgQeUvqGkhFGkmVLbCSq3"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("4G35PHXZdYSRyT7A3EJfwQVgQRCts9TgKxXjmxsge3hd"),
					const_hash("7i7fGj7WPFGo92duNHfPiiipGGRqjGM7iwLUGDqqX3Gf"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("3xNwTebfQMHcMWBjMUQgC28LcPWsVkxUdo2BPhoZKfhN"),
					const_hash("6PXxFmkAPq2uQddhvTVGifjZTbGa9PUozApGD5wRhmk3"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("CiB2wJA9mRFNEXpTbshom9444uq36DaYwBdFD6mNchr1"),
					const_hash("Hp81ANt7aeqUkQJK3RZYmiHxb2ZybrzRjqv6zgQePyQp"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("BMLnePDZyjZ1vFuMeuupyf7fk8khwDBqnrWxkVEVRpK8"),
					const_hash("5rBk5hnUdWAfjJzdtzrgCn3bKsmTVfGtiMRVAvsbw8so"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("4xXYUs1gLQtJfYYVeACLT1cyp7nXUAFCyh1viwBnpDeC"),
					const_hash("CH5jcwQgnzvZmuPnA25JSfqeHgb91gQq4WBTXJ66as5A"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("GtcdEqFE5oRyLhaNdvuhGeW9eTJ4Daa4Hv2WWmxDDVr1"),
					const_hash("3gnzN5hhdWc8HpRyac2cMhqujy9SUFYpiNQiR3MTmXyj"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("ERWBS6yjvHxTJznmgcvrLErC6rFNSLmevFy84a3TLBd"),
					const_hash("CC1RTXKx3cFk4ELzkXYNrqvByvb2pKG8rJyJMSqhz2pf"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("8HtyqYYXhQWDkanGK5wcmqQHDwCLyJmZo5928kVQ2GrS"),
					const_hash("B1HnwAMSsuas8jiwN1VFLhzPfRKqEuezFmBSLCy5Fi85"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("4agjA9Fu11dSX7m99EXiVA3KZTkgfB5EasL2ffjcBcct"),
					const_hash("FqLbh1giovgRN2b5hSjZWRn9kGUZ5rPoUVby8okzRmxJ"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("DX4BQoMGzqRu3HMheJhbHWL7jq5tiP8w8C4Wz17byX6H"),
					const_hash("CT4NjpyL8gA4M3UYc6qMnAL34Qw5G3mtTFXvo9UC5WhW"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("74nJ8cohZk1PH89kCBxpZP6dMfccs4f2UWdwK4q2frqx"),
					const_hash("FbzED6uwpZC6XLBTdhUUkprY1utRaCDVJzUjvz7qWUwv"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("CoXNoXnaeHq8X8N1tzh3Av1ykqiWH6ZqFXVL5ta2ktWm"),
					const_hash("DCKvUH3EPgau8DPpRk4nGCxaDCTwcugSYVCAumCcbQmB"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("Awbe9xyu4qDpZeKSQeGJSJpgJQAk1iUzL86ZQSMCHDo7"),
					const_hash("2xGsZqgDf6Fwpui8k1okksbxpdxF1WTKVbLeCqj5D5j7"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("7z7pPPNaHkQj6XyeNbwxnzn4uCmSYgdXSzx5XwcYSVKY"),
					const_hash("5xb1x2eWC6cweh13NjuWVUFZPTCQsGax7XTnVwGew6Jv"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("HzqQwC6LJXMxz2sYqdxKBp9NvySZ9uJJYXLAX4KPSims"),
					const_hash("Cbmh3FBHpa12LxCpJ2mPfPQqD3akUMtomEmawacFndmU"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("4EtSnoG2nVsp3o5exSm1rvWtJwCZfZTbCpnB4MaEKemM"),
					const_hash("HFZZXeRgCzZbEfTUhTZ5oaE3fCyFUaFpErmDJHtCzAmS"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("7LpBjm9SxR8aweyx2xkWN4RKEp4KKG7yCLS7rpftjiJP"),
					const_hash("4RAdsxRWwJzaB2oVFQLECQDoAeQuvjAS4GAzof3CsTFD"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("95b6pSdAH92MBfKzgCq9gSUY8sHh1XujgBzhjkFm1Qtu"),
					const_hash("8GBAu6dkdh17eQtxUmqbQMuS83bJHZcWRUkE72XSpMn6"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("B9dg1C8nf8YfKjshoo3mbWRwNV4Q3PLekzYTvoAcy2qr"),
					const_hash("Ekb6UksMr8crgNaz4HX1ENqGHq2PMkW6LJx8ZDG7kAzp"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("ARigks6etiwnis7ch75EUoyLvMgx5CSVFv72kSv5baoA"),
					const_hash("4n4UkF3ejWu6atxDquFKhuECDy1XsnBpAWDWCuKwwDfY"),
				));

				SolanaAvailableNonceAccounts::<T>::append((
					const_address("HtnVN4WsDx1LfSxy1i4v8jcFZjoyWmUas6zAESF17UAZ"),
					const_hash("3oinHHfsRL1TQLNqPNDMsjh5ZCyiy1rAxQsoEwYmzobH"),
				));
			},
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
