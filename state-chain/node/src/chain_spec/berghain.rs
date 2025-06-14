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

pub use super::common::*;
use super::StateChainEnvironment;
use cf_chains::{
	dot::RuntimeVersion,
	sol::{SolAddress, SolHash},
};
use cf_primitives::{AccountId, AccountRole, BlockNumber, FlipBalance, NetworkEnvironment};
use cf_utilities::bs58_array;
use sc_service::ChainType;
use sol_prim::consts::{const_address, const_hash};
use sp_core::H256;
use state_chain_runtime::SetSizeParameters;

pub struct Config;

pub const NETWORK_NAME: &str = "Chainflip-Berghain";
pub const CHAIN_TYPE: ChainType = ChainType::Live;
pub const NETWORK_ENVIRONMENT: NetworkEnvironment = NetworkEnvironment::Mainnet;
pub const PROTOCOL_ID: &str = "flip-berghain";

// These represent approximately 24 hours on mainnet block times
pub const BITCOIN_EXPIRY_BLOCKS: u32 = 24 * 60 / 10;
pub const ETHEREUM_EXPIRY_BLOCKS: u32 = 24 * 3600 / 14;
pub const ARBITRUM_EXPIRY_BLOCKS: u32 = 24 * 3600 * 4;
pub const POLKADOT_EXPIRY_BLOCKS: u32 = 24 * 3600 / 6;
pub const SOLANA_EXPIRY_BLOCKS: u32 = 24 * 3600 * 10 / 4;
pub const ASSETHUB_EXPIRY_BLOCKS: u32 = 24 * 3600 / 12;

pub const ENV: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("826180541412D574cf1336d22c0C0a287822678A"),
	eth_usdc_address: hex_literal::hex!("A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"),
	eth_usdt_address: hex_literal::hex!("dAC17F958D2ee523a2206206994597C13D831ec7"),
	state_chain_gateway_address: hex_literal::hex!("6995Ab7c4D7F4B03f467Cf4c8E920427d9621DBd"),
	eth_key_manager_address: hex_literal::hex!("cd351d3626Dc244730796A3168D315168eBf08Be"),
	eth_vault_address: hex_literal::hex!("F5e10380213880111522dd0efD3dbb45b9f62Bcc"),
	eth_address_checker_address: hex_literal::hex!("79001a5e762f3bEFC8e5871b42F6734e00498920"), /* TODO: PRO-2320 */
	arb_key_manager_address: hex_literal::hex!("BFe612c77C2807Ac5a6A41F84436287578000275"),
	arb_vault_address: hex_literal::hex!("79001a5e762f3bEFC8e5871b42F6734e00498920"),
	arb_usdc_token_address: hex_literal::hex!("af88d065e77c8cC2239327C5EDb3A432268e5831"),
	arb_address_checker_address: hex_literal::hex!("c1B12993f760B654897F0257573202fba13D5481"), /* TODO: PRO-2320 */
	ethereum_chain_id: cf_chains::eth::CHAIN_ID_MAINNET,
	arbitrum_chain_id: cf_chains::arb::CHAIN_ID_MAINNET,
	eth_init_agg_key: hex_literal::hex!(
		"022a1d7efa522ce746bc40a04016178ce38154be1f0537c6957bdeed17057bb955"
	),
	ethereum_deployment_block: 18562942,
	genesis_funding_amount: GENESIS_AUTHORITY_FUNDING,
	min_funding: MIN_FUNDING,
	dot_genesis_hash: H256(hex_literal::hex!(
		"91b171bb158e2d3848fa23a9f1c25182fb8e20313b2c1eb49219da7a70ce90c3" // Polkadot mainnet
	)),
	dot_vault_account_id: None,
	dot_runtime_version: RuntimeVersion { spec_version: 9431, transaction_version: 24 },
	hub_genesis_hash: H256(hex_literal::hex!(
		"68d56f15f85d3136970ec16946040bc1752654e906147f7e43e9d539d7c3de2f" // Assethub mainnet
	)),
	hub_vault_account_id: None,
	hub_runtime_version: RuntimeVersion { spec_version: 1003004, transaction_version: 15 },
	sol_genesis_hash: Some(SolHash(bs58_array("5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d"))),
	sol_vault_program: SolAddress(bs58_array("AusZPVXPoUM8QJJ2SL4KwvRGCQ22cDg6Y4rg7EvFrxi7")),
	sol_vault_program_data_account: SolAddress(bs58_array(
		"ACLMuTFvDAb3oecQQGkTVqpUbhCKHG3EZ9uNXHK1W9ka",
	)),
	sol_usdc_token_mint_pubkey: SolAddress(bs58_array(
		"EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
	)),
	sol_token_vault_pda_account: SolAddress(bs58_array(
		"4ZhKJgotJ2tmpYs9Y2NkgJzS7Ac5sghrU4a6cyTLEe7U",
	)),
	sol_usdc_token_vault_ata: SolAddress(bs58_array(
		"8KNqCBB1LKWbtjNxY9v2g1fSBKm2ZRgNNv7rmx2bE6Ce",
	)),
	sol_durable_nonces_and_accounts: [
		(
			const_address("BDKywh4jrvMEFRUkX1bzK8JoyXBY7cmjaZh7bRFpMX4o"),
			const_hash("3pMDqkhnibuv2ARQzjq4K1jn58EvCzC6uF28kiMCUoW2"),
		),
		(
			const_address("2sZp8mnaNZW5FLpbys4rG7RCpVWixmWydRyJmPzNgxi4"),
			const_hash("5rJzAL24yzaqPNE14xFuhdLtLLmUDF3JAfVbZHoBWAUB"),
		),
		(
			const_address("J4Afw1uLrnsQQwEUHQiPe71H3Y3gJQ1oZer5q1QBMViC"),
			const_hash("6ChBdxfZ4ZPLZ7zhVavtjXZrNojg1MdT3Du4VnSnhQ6u"),
		),
		(
			const_address("CCqwvHKHuUSRxxbV7RLSnSYt7XaFrQtFEmaVumLVNmJK"),
			const_hash("6FtBFnt4P25xUnATE6c6XeicKn1ZB6Q5MiGZq4xqqD2D"),
		),
		(
			const_address("3bVqyf58hQHsxbjnqnSkopnoyEHB9v9KQwhZj7h1DucW"),
			const_hash("8UZPPjjKVVjb7TRDx3ZVBnMqrdqYpp6HP2vQVUrxEhn1"),
		),
		(
			const_address("5iKkv5RTvHKzn4VdYLWu48dYsPz5tVniUEa3wHrG9hjB"),
			const_hash("FtLgEitvpnSrcj4adHKcvbYG9SF1C7NLZCk2priDTA6e"),
		),
		(
			const_address("3GGKqshYCGcnQKp6iNh8kb5nbZwtNKSbA9Y7H11eAgyU"),
			const_hash("2xYo9Gv76GGgZs2ikCi8gSgkriEugv5wFhERowyvDx3H"),
		),
		(
			const_address("A2mR1Ytk7R8kGvnRxVLTurZzGr9FwvD8A2ovt3ZRCwQS"),
			const_hash("79NHKfzzZZ4Fmm5mK7D6E16KvwJKWWZpuqyHHiD1xdQ3"),
		),
		(
			const_address("HS6RiBAt9FbC62xJ6kLAH4ekpCW8ZE7HiuHZKNaUbk7a"),
			const_hash("CTyzyX8K9Wwo5zGEZmWxtGpYYwHpGv6YTsFRpi6syLJ4"),
		),
		(
			const_address("14AwUr3FG75E66aaLy7jCbVGaxGCGLdqtpVyBNAFwKac"),
			const_hash("AEzmj9wq8jp7wF46Lrr3Jc2K7xRP58V5Y3cYRVEqtE5J"),
		),
		(
			const_address("Gvn18oy3EZydPGmowvSZYVQSA3Tt1tpAF1D7sdS4oqmc"),
			const_hash("6UEw8EEzB4ttJgq3kJvFn1iLTk18KvSJ8Md1vxiEkZqt"),
		),
		(
			const_address("EGq7QuR2tjwEWTT2dqBgbH911cSzCQrosw5fawhv1Ynb"),
			const_hash("6S18X45DpiABYgSxCgVTukUokf3Q3ZemkD8NF1gFfCmz"),
		),
		(
			const_address("Hh4pQvB4CvyVsDf3HY2CPW7RXNihUKnnWdY6Q3Et7XC7"),
			const_hash("Dp6yaqEvMbsZuiL3E8jdyTqwSpwir5B42twjxLThR8oi"),
		),
		(
			const_address("75iFAcFXJL2NUoAGdK4x2L3J6HzKGkphiQV4t7WBWRkG"),
			const_hash("5xqqeUMSqpmQ1M5W2itgc47HQGoQLeV3uKZGoVECtUBX"),
		),
		(
			const_address("F8Sje34REeRP7GenCkA6aWAaZr23MDd37sZnP95fnWUZ"),
			const_hash("Cs35kXNUMCMgoqfdcTyKuRfqGVTbEGVDAwPZidsYgYyn"),
		),
		(
			const_address("441a8gL3LWFunH4FzmSvPdnXdSAf5uAk9G83UM3trSLs"),
			const_hash("DFoiT9qfUjMhxp4UgeTmfDj3iHTW8sGAPjKuPzusc6Vn"),
		),
		(
			const_address("9q3kbdHjAUvFtM7scz6FFoUZpQsPD6Ynv47hPcpaLHcg"),
			const_hash("DJ7BJaMyEyG8LUczagLPCdtQ6yGkv6wam1Gxqy1f99Y3"),
		),
		(
			const_address("DfhR7xCzGsFiEW35dNEEa28MiRx8ST2AENYLKRf7DQs1"),
			const_hash("AtVm1fT2qgxZTSdbj2st6u3AvbW25mzSvbpKt8YiZFDw"),
		),
		(
			const_address("DviGUYND6o5LsmBAcuVJDb3R5ZNwNs4PCieDCQ7W1oxG"),
			const_hash("JAPsKsCv2FquBzgVxw7hbHw7xJqm99zANCZFXrvd83GL"),
		),
		(
			const_address("FDVTwERCAq2X6GDczSmf8mg3Hk8G2Wsjupw75QjpryfM"),
			const_hash("5sHmsBrbBQqhJyZJRdDr7Mgf27XegeLCf9W1YwCKB1n2"),
		),
		(
			const_address("5ibkrb5M6Kd2MYTfobY5oeLPp8QXKTnKJLKhqsVKXfWF"),
			const_hash("4DAJ9Gt1DXoMUyuEbeWT4k5ms672TRWi5rLchoj9KRty"),
		),
		(
			const_address("2dLkhYpCtmyXufU5msW3THSjmGGUTnAoziT3ukBTKQbu"),
			const_hash("2bs5XUHdNQvH4zuDY19WasgmnpDKoHdRQ7LQk1W4Gv2M"),
		),
		(
			const_address("Fzh6VuMyoQDj9it51joLf1zDV6GAzjqXE7aXUN7DDkDi"),
			const_hash("CAVfhDy4vWjZ7TLAs92PTKRE5cywQDyKnex3XCD3pcYf"),
		),
		(
			const_address("HnH4nSudUL7JAjtLrPxPX5adRFR7HJTtCkrA5d5NvLP3"),
			const_hash("8QTuQLRST5bcpjy8yhqDsuDUA1m82VKW51wKqpUkbohF"),
		),
		(
			const_address("7TpvwcE7tXuUkrYJ1JbVsq6r1bH3ea2i9nmEuZWXgBfp"),
			const_hash("8xMcfW95iis3QmwRed29XDS5fPZSkRXGTJkkdLj43ZA3"),
		),
		(
			const_address("FEzs883Xc3j6pk9e4Daox9Yd78h3Xxx9u9ub2GbdK4Fi"),
			const_hash("H5mMuFNGbBmTnxRA1ngFqvqHFSStBfqNwwSF29LmRiaz"),
		),
		(
			const_address("Cr9fHbshkweGk6z4DqVg1FG8dNoWfrgYrsJShpY4L15V"),
			const_hash("71ypT5pKT5q6qfYFLjUBiYsAWKGYuyKka5X8db3oz26q"),
		),
		(
			const_address("HdYL4KEULHN2NKunFPChcGFUYgvqpnZSkLUZUbta1ADj"),
			const_hash("FZoG4f465iN96rVgGZ5rLEf9SqS3cVUifzMMk9enZ2Zr"),
		),
		(
			const_address("82vjKymhRCzSTyULmYnoxXkB5WdH69WT3tVX5gPbap77"),
			const_hash("8ZbEvSSWyi2DM3BEU8wYna17S3hp8N7vJbKPhzhurh6c"),
		),
		(
			const_address("7kCR5ExCF9r9Tk84aqvXWvBWGv1r3X96W5XmiBbGE1yA"),
			const_hash("2MWDZKzHjiugxprfnRLrVVc9nvEEMnu1k4EggPSUYNLn"),
		),
		(
			const_address("6oiAkK8o3KL3w8pbMJpv99jnuuNi57BdB6HqThjgkpB7"),
			const_hash("B5jYinLvxL8nELQtA746Nw73EyD4Fa6Hp7sDYQSQDssC"),
		),
		(
			const_address("3rPs5e1risNQUY8Bh1HVUwFzs89qCGTGY7nvby12PHxt"),
			const_hash("9ep23qh2YBYNR33WK1vJhGVCVER2CjjWa8kTYQdVfH4B"),
		),
		(
			const_address("GknzUHNX8qF3AsHY1dvdDMa2tN3Fua6kCkpKpLzvC7pc"),
			const_hash("Civh5UjRPVn5Gz2QGh9fmHQRBSoQB5PB7rsYW62xbUhw"),
		),
		(
			const_address("6qMTRduNAJeofRhyxPNYK5LgaYgLsgBve3r7aH2Jg5BZ"),
			const_hash("8WXexE58i5kdTeDJmsRzdWc3dHtRSZYnwxiF21m8HTnG"),
		),
		(
			const_address("9korXEj8h9o4GNwju9DTk8HCXKCQfafR5jB1a5gykjcF"),
			const_hash("HAN7Tn2CoKPpogEKDPfW5CCFEDqAdEUGWpfe55s5HPKu"),
		),
		(
			const_address("EXvDJycst46TtzW4p2J6xzMJcAGoNDYno5fiCuxoNAMd"),
			const_hash("8w2vHaXYibhTW7Y9AFJhLEmJS3xR4wvkASJ8CGUpuZ49"),
		),
		(
			const_address("5w7Efx8GR1fJ1cKPgTsK7rzeu1T2VqfQXPNgtPfsKfW8"),
			const_hash("ZV8Z1dgDHrPJocsqy9kPdHRMxHNfLzTBdEUNmCAJktP"),
		),
		(
			const_address("G4ntoKddtNrBu9TiAqBGmrboY54fCDDKg2S8M822xuwS"),
			const_hash("64oEUGLNVeEg8S8Y3NBUesNGUbps75tiChpQzb4E5rrQ"),
		),
		(
			const_address("DW2en8bGK7C9yxtBaEniLbCaYm5hETG79LEsLK4cXCxs"),
			const_hash("8JaGEpABTpsMehyUB4Rtsfjxf6oWJFTR6RKkMQexrKA1"),
		),
		(
			const_address("4hXQXaS4nyGLS2LBV5Z4iWyAs61LP4DCTkr66oUV8qqk"),
			const_hash("BVgTfnLxkedAouCu9aaadReDHsd4ibQu4tjPVxK73Y2n"),
		),
		(
			const_address("3AprWhiw1EGF1SsayNbCQDj7DV5BZ9naRSBB2LS6GzAD"),
			const_hash("2Bnj8PdTbGpgwQCGB3PySoXaS57gYhVWt3XoStYV1HBP"),
		),
		(
			const_address("BL6h9KaC1RJZuAD7mK1TtwJXQJ5YQC3zcD9xEvWMguQ4"),
			const_hash("e6pGcrkMnhBY1aLS1EzCJrSKtJED2dps2PPCYUCapB5"),
		),
		(
			const_address("e8t1kzNkwvU7fczonysbGU6HkKuQSHKfDHWhACuH9VL"),
			const_hash("A5outvSBwTVRL4wkwTxLGj2JM7nUv68afmvL44Dq6PEp"),
		),
		(
			const_address("G29wnM1eQrHqZeV43GvrEf5RsK2VbjecnS94S7bCGSP3"),
			const_hash("9B43eXxdApVRT4u129u3Qxp749H777JLtCeR5UTzncch"),
		),
		(
			const_address("DefhiKiD54tGa1LTpwM4LevYtUenGUi36UkBkQqAfvet"),
			const_hash("Brg5UziLTWU2ouxchyURSX53MTbRbYjoAFTCR2inyVr8"),
		),
		(
			const_address("69HunmA67d16zAK14biGGAZMiMqn9qBJCfxcRAjJMm3E"),
			const_hash("71UJG6yxZzV9QppATPReRqM7oNY1UbxJtaWgZa2koY3h"),
		),
		(
			const_address("8WawSHgiexniQ52H7KcxqaxQpknCYythW6ghPCfWfj62"),
			const_hash("3csxLEwWzMjWKNfHy1Rraym7QXNfpGunmgYNJckYc89n"),
		),
		(
			const_address("GgtqYVQdaUj1k6WSnyYfEHUMy7Nf8jxze2jy8UenuNwE"),
			const_hash("Goh4K5Y4TVbhyThuQRyAe1Ghh8gjMn9oujzejgupmz3w"),
		),
		(
			const_address("6LihAbvnT46GXKfjqQaPPiB7nadusCrFyyY1Pe5A6m3W"),
			const_hash("BvsLHsz8GnGdnpY6r8PZwp3w3FUrb6kpP5V2yNU2Jgcd"),
		),
		(
			const_address("CeTTyF33ZDijNs9MCCMZVtoUFi5WBFVmZVrEkhoWhiHc"),
			const_hash("BRVa4YUgwLrXnJDiuRtBt8yihWA4PPchEMpMGopCPRtr"),
		),
	],
	sol_swap_endpoint_program: SolAddress(bs58_array(
		"J88B7gmadHzTNGiy54c9Ms8BsEXNdB2fntFyhKpk3qoT",
	)),
	sol_swap_endpoint_program_data_account: SolAddress(bs58_array(
		"FmAcjWaRFUxGWBfGT7G3CzcFeJFsewQ4KPJVG4f6fcob",
	)),
	sol_alt_manager_program: SolAddress(bs58_array("9WuKqnyB4i7sFh2VkrPSW4dkJ3pB8wePQ9MeHEUudsvV")),
	sol_address_lookup_table_account: (
		SolAddress(bs58_array("CFdyxyubce52nvCVVP3XEgCyJmxMVg5yf4VV41aybh9k")),
		[
			const_address("ACLMuTFvDAb3oecQQGkTVqpUbhCKHG3EZ9uNXHK1W9ka"),
			const_address("SysvarRecentB1ockHashes11111111111111111111"),
			const_address("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"),
			const_address("4ZhKJgotJ2tmpYs9Y2NkgJzS7Ac5sghrU4a6cyTLEe7U"),
			const_address("8KNqCBB1LKWbtjNxY9v2g1fSBKm2ZRgNNv7rmx2bE6Ce"),
			const_address("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"),
			const_address("Sysvar1nstructions1111111111111111111111111"),
			const_address("FmAcjWaRFUxGWBfGT7G3CzcFeJFsewQ4KPJVG4f6fcob"),
			const_address("3tJ67qa2GDfvv2wcMYNUfN5QBZrFpTwcU8ASZKMvCTVU"),
			const_address("So11111111111111111111111111111111111111112"),
			const_address("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"),
			const_address("11111111111111111111111111111111"),
			const_address("AusZPVXPoUM8QJJ2SL4KwvRGCQ22cDg6Y4rg7EvFrxi7"),
			const_address("J88B7gmadHzTNGiy54c9Ms8BsEXNdB2fntFyhKpk3qoT"),
			const_address("9WuKqnyB4i7sFh2VkrPSW4dkJ3pB8wePQ9MeHEUudsvV"),
			const_address("BDKywh4jrvMEFRUkX1bzK8JoyXBY7cmjaZh7bRFpMX4o"),
			const_address("2sZp8mnaNZW5FLpbys4rG7RCpVWixmWydRyJmPzNgxi4"),
			const_address("J4Afw1uLrnsQQwEUHQiPe71H3Y3gJQ1oZer5q1QBMViC"),
			const_address("CCqwvHKHuUSRxxbV7RLSnSYt7XaFrQtFEmaVumLVNmJK"),
			const_address("3bVqyf58hQHsxbjnqnSkopnoyEHB9v9KQwhZj7h1DucW"),
			const_address("5iKkv5RTvHKzn4VdYLWu48dYsPz5tVniUEa3wHrG9hjB"),
			const_address("3GGKqshYCGcnQKp6iNh8kb5nbZwtNKSbA9Y7H11eAgyU"),
			const_address("A2mR1Ytk7R8kGvnRxVLTurZzGr9FwvD8A2ovt3ZRCwQS"),
			const_address("HS6RiBAt9FbC62xJ6kLAH4ekpCW8ZE7HiuHZKNaUbk7a"),
			const_address("14AwUr3FG75E66aaLy7jCbVGaxGCGLdqtpVyBNAFwKac"),
			const_address("Gvn18oy3EZydPGmowvSZYVQSA3Tt1tpAF1D7sdS4oqmc"),
			const_address("EGq7QuR2tjwEWTT2dqBgbH911cSzCQrosw5fawhv1Ynb"),
			const_address("Hh4pQvB4CvyVsDf3HY2CPW7RXNihUKnnWdY6Q3Et7XC7"),
			const_address("75iFAcFXJL2NUoAGdK4x2L3J6HzKGkphiQV4t7WBWRkG"),
			const_address("F8Sje34REeRP7GenCkA6aWAaZr23MDd37sZnP95fnWUZ"),
			const_address("441a8gL3LWFunH4FzmSvPdnXdSAf5uAk9G83UM3trSLs"),
			const_address("9q3kbdHjAUvFtM7scz6FFoUZpQsPD6Ynv47hPcpaLHcg"),
			const_address("DfhR7xCzGsFiEW35dNEEa28MiRx8ST2AENYLKRf7DQs1"),
			const_address("DviGUYND6o5LsmBAcuVJDb3R5ZNwNs4PCieDCQ7W1oxG"),
			const_address("FDVTwERCAq2X6GDczSmf8mg3Hk8G2Wsjupw75QjpryfM"),
			const_address("5ibkrb5M6Kd2MYTfobY5oeLPp8QXKTnKJLKhqsVKXfWF"),
			const_address("2dLkhYpCtmyXufU5msW3THSjmGGUTnAoziT3ukBTKQbu"),
			const_address("Fzh6VuMyoQDj9it51joLf1zDV6GAzjqXE7aXUN7DDkDi"),
			const_address("HnH4nSudUL7JAjtLrPxPX5adRFR7HJTtCkrA5d5NvLP3"),
			const_address("7TpvwcE7tXuUkrYJ1JbVsq6r1bH3ea2i9nmEuZWXgBfp"),
			const_address("FEzs883Xc3j6pk9e4Daox9Yd78h3Xxx9u9ub2GbdK4Fi"),
			const_address("Cr9fHbshkweGk6z4DqVg1FG8dNoWfrgYrsJShpY4L15V"),
			const_address("HdYL4KEULHN2NKunFPChcGFUYgvqpnZSkLUZUbta1ADj"),
			const_address("82vjKymhRCzSTyULmYnoxXkB5WdH69WT3tVX5gPbap77"),
			const_address("7kCR5ExCF9r9Tk84aqvXWvBWGv1r3X96W5XmiBbGE1yA"),
			const_address("6oiAkK8o3KL3w8pbMJpv99jnuuNi57BdB6HqThjgkpB7"),
			const_address("3rPs5e1risNQUY8Bh1HVUwFzs89qCGTGY7nvby12PHxt"),
			const_address("GknzUHNX8qF3AsHY1dvdDMa2tN3Fua6kCkpKpLzvC7pc"),
			const_address("6qMTRduNAJeofRhyxPNYK5LgaYgLsgBve3r7aH2Jg5BZ"),
			const_address("9korXEj8h9o4GNwju9DTk8HCXKCQfafR5jB1a5gykjcF"),
			const_address("EXvDJycst46TtzW4p2J6xzMJcAGoNDYno5fiCuxoNAMd"),
			const_address("5w7Efx8GR1fJ1cKPgTsK7rzeu1T2VqfQXPNgtPfsKfW8"),
			const_address("G4ntoKddtNrBu9TiAqBGmrboY54fCDDKg2S8M822xuwS"),
			const_address("DW2en8bGK7C9yxtBaEniLbCaYm5hETG79LEsLK4cXCxs"),
			const_address("4hXQXaS4nyGLS2LBV5Z4iWyAs61LP4DCTkr66oUV8qqk"),
			const_address("3AprWhiw1EGF1SsayNbCQDj7DV5BZ9naRSBB2LS6GzAD"),
			const_address("BL6h9KaC1RJZuAD7mK1TtwJXQJ5YQC3zcD9xEvWMguQ4"),
			const_address("e8t1kzNkwvU7fczonysbGU6HkKuQSHKfDHWhACuH9VL"),
			const_address("G29wnM1eQrHqZeV43GvrEf5RsK2VbjecnS94S7bCGSP3"),
			const_address("DefhiKiD54tGa1LTpwM4LevYtUenGUi36UkBkQqAfvet"),
			const_address("69HunmA67d16zAK14biGGAZMiMqn9qBJCfxcRAjJMm3E"),
			const_address("8WawSHgiexniQ52H7KcxqaxQpknCYythW6ghPCfWfj62"),
			const_address("GgtqYVQdaUj1k6WSnyYfEHUMy7Nf8jxze2jy8UenuNwE"),
			const_address("6LihAbvnT46GXKfjqQaPPiB7nadusCrFyyY1Pe5A6m3W"),
			const_address("CeTTyF33ZDijNs9MCCMZVtoUFi5WBFVmZVrEkhoWhiHc"),
		],
	),
};

pub const EPOCH_DURATION_BLOCKS: BlockNumber = 24 * HOURS;

pub const BASHFUL_ACCOUNT_ID: &str = "cFNzzoURRFHx2fw2EmsCvTc7hBFP34EaP2B23oUcFdbp1FMvx";
pub const BASHFUL_SR25519: [u8; 32] =
	hex_literal::hex!["e2e8c8d8a2662d11a96ab6cbf8f627e78d6c77ac011ad0ad65b704976c7c5b6c"];
pub const BASHFUL_ED25519: [u8; 32] =
	hex_literal::hex!["c2729cfb8507558af71474e9610071585e4ae02c5418e053cdc25106628f9810"];
pub const DOC_ACCOUNT_ID: &str = "cFP2cGErEhxzJfVUxk1gHVuE1ALxHJQx335o19bT7QoSWwjhU";
pub const DOC_SR25519: [u8; 32] =
	hex_literal::hex!["e42367696495e88be9b78e7e639bc0a870139bfe43aafb46ea5f934c69903b02"];
pub const DOC_ED25519: [u8; 32] =
	hex_literal::hex!["5e52d11949673e9ba3a6e3e11c0fc0537bc588de8ac61d41cf04e0ff43dc39a1"];
pub const DOPEY_ACCOUNT_ID: &str = "cFKzr7DwLCRtSkou5H5moKri7g9WwJ4tAbVJv6dZGhLb811Tc";
pub const DOPEY_SR25519: [u8; 32] =
	hex_literal::hex!["5e16d155cf85815a0ba8957762e1e007eec4d5c6fe0b32b4719ca4435c36eb57"];
pub const DOPEY_ED25519: [u8; 32] =
	hex_literal::hex!["99cca386ea50fb33d2eee5ebd5574759facb17ddd55241e246b59567f6878242"];
pub const SNOW_WHITE_ACCOUNT_ID: &str = "cFPVXzCyCxKbxJEHhDN1yXrU3VcDPZswHSVHh8HnoGsJsAVYS";
pub const SNOW_WHITE_SR25519: [u8; 32] =
	hex_literal::hex!["f8aca257e6ab69e357984a885121c0ee18fcc50185c77966cdaf063df2f89126"];

pub fn extra_accounts() -> Vec<(AccountId, AccountRole, FlipBalance, Option<Vec<u8>>)> {
	vec![]
}

// Set to zero initially, will be updated by governance to 7% / 1% annual.
pub const CURRENT_AUTHORITY_EMISSION_INFLATION_PERBILL: u32 = 0;
pub const BACKUP_NODE_EMISSION_INFLATION_PERBILL: u32 = 0;

pub const SUPPLY_UPDATE_INTERVAL: u32 = 30 * 24 * HOURS;

pub const MIN_FUNDING: FlipBalance = 6 * FLIPPERINOS_PER_FLIP;
pub const GENESIS_AUTHORITY_FUNDING: FlipBalance = 1_000 * FLIPPERINOS_PER_FLIP;
pub const REDEMPTION_TAX: FlipBalance = 5 * FLIPPERINOS_PER_FLIP;

/// Redemption delay on mainnet is 48 HOURS.
/// We add an extra 24 hours buffer.
pub const REDEMPTION_TTL_SECS: u64 = (48 + 24) * 3600;

pub const AUCTION_PARAMETERS: SetSizeParameters =
	SetSizeParameters { min_size: 3, max_size: MAX_AUTHORITIES, max_expansion: MAX_AUTHORITIES };

pub const BITCOIN_SAFETY_MARGIN: u64 = 2;
pub const ETHEREUM_SAFETY_MARGIN: u64 = 6;
pub const ARBITRUM_SAFETY_MARGIN: u64 = 1;
pub const SOLANA_SAFETY_MARGIN: u64 = 1; // Unused - we use "finalized" instead
