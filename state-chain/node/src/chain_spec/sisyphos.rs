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

use super::StateChainEnvironment;
pub use super::{
	common::*,
	testnet::{
		ARBITRUM_EXPIRY_BLOCKS, ASSETHUB_EXPIRY_BLOCKS, BITCOIN_EXPIRY_BLOCKS,
		ETHEREUM_EXPIRY_BLOCKS, POLKADOT_EXPIRY_BLOCKS, SOLANA_EXPIRY_BLOCKS,
	},
};
use cf_chains::{
	dot::RuntimeVersion,
	sol::{SolAddress, SolHash},
};
use cf_primitives::{
	AccountId, AccountRole, BlockNumber, ChainflipNetwork, FlipBalance, NetworkEnvironment,
};
use cf_utilities::bs58_array;
use pallet_cf_elections::generic_tools::Array;
use sc_service::ChainType;
use sp_core::{H160, H256};

use sol_prim::consts::{const_address, const_hash};
use state_chain_runtime::chainflip::generic_elections::ChainlinkOraclePriceSettings;

pub struct Config;

pub const NETWORK_NAME: &str = "Chainflip-Sisyphos";
pub const CHAIN_TYPE: ChainType = ChainType::Live;
pub const NETWORK_ENVIRONMENT: NetworkEnvironment = NetworkEnvironment::Testnet;
pub const CHAINFLIP_NETWORK: ChainflipNetwork = ChainflipNetwork::TestnetDev;
pub const PROTOCOL_ID: &str = "flip-sisy-2";

pub const GENESIS_FUNDING_AMOUNT: FlipBalance = 1_000 * FLIPPERINOS_PER_FLIP;

pub const ENV: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("cD079EAB6B5443b545788Fd210C8800FEADd87fa"),
	eth_usdc_address: hex_literal::hex!("1c7D4B196Cb0C7B01d743Fbc6116a902379C7238"),
	eth_usdt_address: hex_literal::hex!("27cea6eb8a21aae05eb29c91c5ca10592892f584"),
	eth_wbtc_address: hex_literal::hex!("b060796D171EeEdA5Fb99df6B2847DA6D4613CAd"), //TODO change
	state_chain_gateway_address: hex_literal::hex!("1F7fE41C798cc7b1D34BdC8de2dDDA4a4bE744D9"),
	eth_key_manager_address: hex_literal::hex!("22f5562e6859924Db082b8B248ea0C974f148a17"),
	eth_vault_address: hex_literal::hex!("a94d6b1853F3cb611Ed3cCb701b4fdA5a9DACe85"),
	eth_address_checker_address: hex_literal::hex!("26061f315570bddF11D9055411a3d811c5FF0148"),
	eth_sc_utils_address: hex_literal::hex!("7c08ea651dA70239DA8cb87A5913c3579Ba9F6fE"),
	arb_key_manager_address: hex_literal::hex!("7EA74208E2954a7294097C731434caD29c5094D8"),
	arb_vault_address: hex_literal::hex!("8155BdD48CD011e1118b51A1C82be020A3E5c2f2"),
	arb_usdc_token_address: hex_literal::hex!("75faf114eafb1BDbe2F0316DF893fd58CE46AA4d"),
	arb_address_checker_address: hex_literal::hex!("564e411634189E68ecD570400eBCF783b4aF8688"),
	ethereum_chain_id: cf_chains::eth::CHAIN_ID_SEPOLIA,
	arbitrum_chain_id: cf_chains::arb::CHAIN_ID_ARBITRUM_SEPOLIA,
	eth_init_agg_key: hex_literal::hex!(
		"025e790770ed8e79c08d68fa781b2848651f3e94ef8b1305a7fb6de782798735ad"
	),
	sol_init_agg_key: None,
	ethereum_deployment_block: 5429873u64,
	genesis_funding_amount: GENESIS_FUNDING_AMOUNT,
	min_funding: MIN_FUNDING,
	dot_genesis_hash: H256(hex_literal::hex!(
		"5a7ebe8e4d69752907aef5a79e1908e2ceadd7f91cbe1e424d80621f7916ea24"
	)),
	dot_vault_account_id: None,
	dot_runtime_version: RuntimeVersion { spec_version: 10000, transaction_version: 25 },
	hub_genesis_hash: H256(hex_literal::hex!(
		"d6ca94b515c4693ca4acc8a04afa935572c2896a796b691848f075d5749c6afc"
	)),
	hub_vault_account_id: None,
	hub_runtime_version: RuntimeVersion { spec_version: 1003004, transaction_version: 15 },
	sol_genesis_hash: Some(SolHash(bs58_array("EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG"))),
	sol_vault_program: SolAddress(bs58_array("Gvcsg1ADZJSFXFRp7RUR1Z3DtMZec8iWUPoPVCMv4VQh")),
	sol_vault_program_data_account: SolAddress(bs58_array(
		"DXF45ndZRWkHQvQcFdLuNmT3KHP18VCshJK1mQoLUAWz",
	)),
	sol_usdc_token_mint_pubkey: SolAddress(bs58_array(
		"4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU",
	)),
	sol_token_vault_pda_account: SolAddress(bs58_array(
		"FsQeQkrTWETD8wbZhKyQVfWQLjprjdRG8GAriauXn972",
	)),
	sol_usdc_token_vault_ata: SolAddress(bs58_array(
		"B2d8rCk5jXUfjgYMpVRARQqZ4xh49XNMf7GYUFtdZd6q",
	)),
	sol_durable_nonces_and_accounts: [
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
		(
			const_address("5xj8KDCVGLPvXMjeUjvnekTZUh1ojyfJUaYdmP4augDj"),
			const_hash("6HzAWG8d1AQonZ3pwLWJV9WrWYgwxnUJY2GhgttJKkcA"),
		),
		(
			const_address("pdyvBtXeDVxGWDs6kJjLtQf8PmLZbhUTyoz2ogohavu"),
			const_hash("4hrU2kCAk6a74dZisvVr1ZWkJhPVjQAVCL9mA8NzWu7z"),
		),
		(
			const_address("34jPE4S9PupsyqcQ13av6wg7MzsntRvNsq72woipCXG5"),
			const_hash("8HQRmyAmGkBQUEyBDLhb8jPjy1uixksUz91CcFc6JyK8"),
		),
		(
			const_address("FNVLBq9FMfsUBTbJjULMENtg6sLtNb74LvjaXZfFanRF"),
			const_hash("7MziAmsVQKffKjQ3RoJXnJqJ2F49bGQndhHaAHDXoi8K"),
		),
		(
			const_address("BBadHPGbJJSAWZgYgXdTZGrYYT6SrqRVJyg9JUGrQbnx"),
			const_hash("41No3gjHRq5ZYX6EgvACfg2pxathmBg1T4sz7pfLY8v1"),
		),
		(
			const_address("Fp38wuVb1sn8usCtN6nzb54R2MwRwVcmV2z63yjabBxP"),
			const_hash("9pgAxFfYWugjraiLoP5NzeMhdmAbB4EQkwKmcfjxG6cG"),
		),
		(
			const_address("3KddDN5PFoMcty9wSfUUZTpfQJadurq1dETPsV3dMf6m"),
			const_hash("47jUSZ7yLSfB17gQHVBZM3bwYxFL8VvWGaTweoXqZtZC"),
		),
		(
			const_address("2fFrkZYHM9ZxkTHqA9yPitVVaxNkTr2Vb93TQdNwfAAG"),
			const_hash("8vkXMgZ16FNjZb68VdtQwp1uzWCnpZJnY91uA8syXNdT"),
		),
		(
			const_address("2H8vKQTSdMe296LTKaQpuXg2jZ9wgwcgUXxL2CY9As7w"),
			const_hash("4UEB4R2oCiaLbMk125KxcRamoPNPHaGWLwBTM8UQGwhC"),
		),
		(
			const_address("FMxKBbsXdgwroxDedrokE3yPJLUg3xMnze8widSgbvaZ"),
			const_hash("BkjLb5HguMDpYVPtFUcNaZyFD1v46HiLu4Bf4MNiEvWb"),
		),
		(
			const_address("GMeTF6WqDAjGBGqLsebVbckZGcvsxbGHEWvLupGrpgZp"),
			const_hash("GyXPxjydxXVrkqN3ETXwSHnzDj4un6wFCorEwGEkR7SU"),
		),
		(
			const_address("2vUVkEPWY2Ckw9Cwtd1WU3htJS6UUQCLoVtzkSey9U5G"),
			const_hash("EYVUL9Ev6Mp57vZLEDsHZF1VCrWQGQTs1fHKCN2jAxQx"),
		),
		(
			const_address("7SKjU5Pdnc5Ux5BAFMN1hEqcVseM71jEGsDqqUgWktv2"),
			const_hash("9g7vZTi7GSjc5WqCUhKXWq3gjGLSy2p4Y3gyVzQizbER"),
		),
		(
			const_address("4ZcUnRpJitLd4yTm9vLd3obCzHG5EYTGZprjkatugXRJ"),
			const_hash("3avMm96VgjwGhMDWHx5kLXgokJELQrRUJgCXEMXrNSdZ"),
		),
		(
			const_address("AhkHTwnDGZjz7kqmAEEGiEUyDKijHsENt3KjzgfiLT6K"),
			const_hash("BVz34q9yTapvLNVDap3kNHwKJUiCZhxWLFHmWdc7BrZ9"),
		),
		(
			const_address("4ABNV5jDexAKxrnUy9XVFyvCtnvSK7M8k1kZRqhdWABf"),
			const_hash("Fc2ni2WuyG9HMN5uAgd2224GVUTmXYoUsBr8nNMLdViB"),
		),
		(
			const_address("9H87SQJn25aVnB8YrnrCZHNwy18AKow1SsBEFM5ubYbh"),
			const_hash("En9FXjhbusvfM8PGwYL8AWeoLizD23fZNTW2QPfLoFKg"),
		),
		(
			const_address("9cmsCRypzNeZ8tEPqSM92jRvjdET1m6J2JkJv9YsNmV5"),
			const_hash("BBpzrANo5dMgucF4YiKW9e2hTyVvLp8LY4tKDjiDpToB"),
		),
		(
			const_address("Du4QkRu2rVwLcFBUJAGQ2DXPHTz6mVfNLNVyid5o6Vm6"),
			const_hash("DJExDrHJgU7hMH2TQ4AEgpSwhQJnEEe7eZgDChPbnBs5"),
		),
		(
			const_address("AZHLvwNcGdZP1AsGHFc2hzs11APJiGbEhkyK5VpyCBKa"),
			const_hash("9f3DfcMBLu3dGy13BkL4ppji5C9ehd3eFkSmMMwMSrTn"),
		),
		(
			const_address("7hVJaSegGTdVtDwZ9iNJyPuSD3HX3iZ9SDdCsqShkypc"),
			const_hash("EEF1GCZd7Fz3E3n29CsKpeFUpFkeKAgSUgAfN4VFCufv"),
		),
		(
			const_address("8NwHCwPfzpyQvQxXTypmw4QQdHxLpZrmuyJ2wBRny2cE"),
			const_hash("3D9qkRrHqShAq52Gh9mhd6EaeKNBi5jAJNtYpzhzz4xd"),
		),
		(
			const_address("FQyP8Pe4xFaeu1wPEaA3nqor3UrtWdFMYTXq4J92JEoZ"),
			const_hash("4yrAPq83REY4mDBdM1t6fRfwVHch9TcX17cvCTLyERYC"),
		),
		(
			const_address("3B3Vwvfx1ZWwcrf1i5F26w4zs7SpMva4JZMnMob8FKvs"),
			const_hash("8oEKxLGFXU9LzXyoDN3B1tfoWSp6VywxKu8xsK2HRkTS"),
		),
		(
			const_address("FRB7dgrjcvvGc4faqhXQyzPwvNBacx7AQoGURiA721q9"),
			const_hash("71JSbJaBQ1HTkexV4kjKAtpLsQ7Pu9hXfwW7jTX4yFbi"),
		),
		(
			const_address("6jGyYPcu1QRfyV7s99QZ5DyaTzsjbvaDcyTNiYfF2j2k"),
			const_hash("HsvC3gJdTqV2u65eh8wCLMHuvyr5KeTrkFoa6D2g1SSf"),
		),
		(
			const_address("CcGQ73N19U5Po99FrcjLsCHLsSdvT276tCmesZckxzrc"),
			const_hash("CK4KzudMWypcGmSpYVQrsLowfrxVSaMYhu2sYMmUrBf8"),
		),
		(
			const_address("7zne7jv6cvTLBaTTMCFvvqXwpMdqwwSdWY58n2v7xXgY"),
			const_hash("8ZBZThmtPS38udebpzec1ZhzV1jdgTov7A3cKEnhioMJ"),
		),
		(
			const_address("FfRe1ZrayiNd4uVrCg8CoWKHvZrdQZqGpSHT9BPMLq5N"),
			const_hash("3z7TyNB45CGHvqiggGsY2zGAB97XgwnptjQNJJLsV6kU"),
		),
		(
			const_address("8xqgHheNm75KgfxXrwTH84vVCJFbRfgiDYikaXLcpEgv"),
			const_hash("HqDEAy8ThkARjvccr74x8XQ68aT7R4RYsPLagBWU5Xmy"),
		),
		(
			const_address("5DrhcUmXwoWLwzeCU3xVhAjg1MHL8JqcpAisX645NPSW"),
			const_hash("AEadjgMmchmwY23hG1sj9qkmVTRywCp3FsszrdDYUJaP"),
		),
		(
			const_address("98ENa65H4azGmaEdn3kx7VMmy5Hx73jZdShAbvQaaTy5"),
			const_hash("EyMjSfSdvR8dy7NGVm88GWo4sm2RA7qAVup4FFYLQf2c"),
		),
		(
			const_address("B1LUePw4D7PwcFqNbbNBSYJjopgBQSV4NYKmEgqNAN5v"),
			const_hash("GWDnjcW7VEyzFh7eDyfLvSUHu8iR7g1ins7URV7qcpMw"),
		),
		(
			const_address("AdKGe6Bv1qFUUzoLv9BQKRn49RCM7sVxrHVy5zniAznn"),
			const_hash("4C24gnmBZVXgSxxtvbuZA2RqJkoYZtP39hTbwajJJDy2"),
		),
		(
			const_address("BQPXeAXL89DcffrdfCpqNNcu5ehQdvHYZL75pLS1GMxg"),
			const_hash("333YxW6k2ib1dCaDW1rZkBCyoZNpFaXWidvPWEm5suG4"),
		),
		(
			const_address("G5xssHyVV1r3bLRastAXnr27cvB3KYBMjwEDD5H4nqxU"),
			const_hash("6fy7NNJYt6tiEJKB177E1pVBrTUhksvQoGEdFGbrm6Rd"),
		),
		(
			const_address("Gj5CfJA4nP6m5xHekk28QRAanJUJEFFx2fjKHUdSagGY"),
			const_hash("4vYWgeUrwDb7PfXoJS9pBLbtXRAug9C6AxwhQeLBb3ta"),
		),
		(
			const_address("G9dvMwe1hJuSGrnqLdkbSnuWH386iL3UuxYuJz64FeLf"),
			const_hash("8wnnU5x6agyiJgKARZmmf2TRYjobxJGLoQz19LaEzV1A"),
		),
		(
			const_address("BuCN3zHPSfy1489ajoiVD3cNstpLMrePyeTs4QAcENyH"),
			const_hash("TNugQtRn4NaC8kFJZaq7zi97mZgC96mCag1j9JBcQdr"),
		),
		(
			const_address("2zMqwgU9xm4foAaHGnYKiWANePwb4bhfYREyU9HSK6Eb"),
			const_hash("NhPfjnNeYwsKT2YwruVVGTNWaJSdgMsxnEwHGZ6cwW2"),
		),
	],
	sol_swap_endpoint_program: SolAddress(bs58_array(
		"FtK6TR2ZqhChxXeDFoVzM9gYDPA18tGrKoBb3hX7nPwt",
	)),
	sol_swap_endpoint_program_data_account: SolAddress(bs58_array(
		"EXeku7Q9AiAXBdH7cUHw2ue3okhrofvDZR7EBE1BVQZu",
	)),
	sol_alt_manager_program: SolAddress(bs58_array("6mDRToYmsEzuTmEZ5SdNcd2y4UDVEZ4xJSFvk4FjnvXG")),
	sol_address_lookup_table_account: (
		SolAddress(bs58_array("Ast7ygd4AMPuy6ZUsk4FnDKCUkdcVR2T9ZQT8aAxveGu")),
		[
			const_address("DXF45ndZRWkHQvQcFdLuNmT3KHP18VCshJK1mQoLUAWz"),
			const_address("SysvarRecentB1ockHashes11111111111111111111"),
			const_address("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"),
			const_address("FsQeQkrTWETD8wbZhKyQVfWQLjprjdRG8GAriauXn972"),
			const_address("B2d8rCk5jXUfjgYMpVRARQqZ4xh49XNMf7GYUFtdZd6q"),
			const_address("4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU"),
			const_address("Sysvar1nstructions1111111111111111111111111"),
			const_address("EXeku7Q9AiAXBdH7cUHw2ue3okhrofvDZR7EBE1BVQZu"),
			const_address("APzLHyWY4CZtTjk5ynxCLW2E2W9R1DY4yFeGNhwSeBzg"),
			const_address("So11111111111111111111111111111111111111112"),
			const_address("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"),
			const_address("11111111111111111111111111111111"),
			const_address("Gvcsg1ADZJSFXFRp7RUR1Z3DtMZec8iWUPoPVCMv4VQh"),
			const_address("FtK6TR2ZqhChxXeDFoVzM9gYDPA18tGrKoBb3hX7nPwt"),
			const_address("6mDRToYmsEzuTmEZ5SdNcd2y4UDVEZ4xJSFvk4FjnvXG"),
			const_address("Cr5YnF9p4M91CrQGHJhP3Syy4aGZNVAwF6zvTxkAZZfj"),
			const_address("3E14JFszKMCDcxXuGk4mDsBddHxSpXrzZ2ZpGHGr8WJv"),
			const_address("C5qNSCcusHvkPrWEt7fQQ8TbgFMkoEetfpigpJEvwam"),
			const_address("FG2Akgw76D5GbQZHpmwPNBSMi3pXq4ffZeYrY7sfUCp4"),
			const_address("HmqRHTmDbQEhkD3RPR58VM6XtF5Gytod5XmgYz9r5Lyx"),
			const_address("FgRZqCYnmjpBY5WA16y73TqRbkLD3zr5btQiSB2B8sr7"),
			const_address("BR7Zn41M6enmL5vcfKHnTzr3F5g6rMAG64uDiZYQ5W3Z"),
			const_address("4TdqxPvxST91mbTyup2Pc87MBhVywpt2T7JQP6bAazsp"),
			const_address("5c4JZKCroL3Sg6Sm7giqdh57cvatJpSPHpmcJX3uJAMm"),
			const_address("DcEmNXySnth2FNhsHmt64oB15pjtakKhfG3nez7qB52w"),
			const_address("5xj8KDCVGLPvXMjeUjvnekTZUh1ojyfJUaYdmP4augDj"),
			const_address("pdyvBtXeDVxGWDs6kJjLtQf8PmLZbhUTyoz2ogohavu"),
			const_address("34jPE4S9PupsyqcQ13av6wg7MzsntRvNsq72woipCXG5"),
			const_address("FNVLBq9FMfsUBTbJjULMENtg6sLtNb74LvjaXZfFanRF"),
			const_address("BBadHPGbJJSAWZgYgXdTZGrYYT6SrqRVJyg9JUGrQbnx"),
			const_address("Fp38wuVb1sn8usCtN6nzb54R2MwRwVcmV2z63yjabBxP"),
			const_address("3KddDN5PFoMcty9wSfUUZTpfQJadurq1dETPsV3dMf6m"),
			const_address("2fFrkZYHM9ZxkTHqA9yPitVVaxNkTr2Vb93TQdNwfAAG"),
			const_address("2H8vKQTSdMe296LTKaQpuXg2jZ9wgwcgUXxL2CY9As7w"),
			const_address("FMxKBbsXdgwroxDedrokE3yPJLUg3xMnze8widSgbvaZ"),
			const_address("GMeTF6WqDAjGBGseqVbckZGcvsxbGHEWvLupGrpgZp"),
			const_address("2vUVkEPWY2Ckw9Cwtd1WU3htJS6UUQCLoVtzkSey9U5G"),
			const_address("7SKjU5Pdnc5Ux5BAFMN1hEqcVseM71jEGsDqqUgWktv2"),
			const_address("4ZcUnRpJitLd4yTm9vLd3obCzHG5EYTGZprjkatugXRJ"),
			const_address("AhkHTwnDGZjz7kqmAEEGiEUyDKijHsENt3KjzgfiLT6K"),
			const_address("4ABNV5jDexAKxrnUy9XVFyvCtnvSK7M8k1kZRqhdWABf"),
			const_address("9H87SQJn25aVnB8YrnrCZHNwy18AKow1SsBEFM5ubYbh"),
			const_address("9cmsCRypzNeZ8tEPqSM92jRvjdET1m6J2JkJv9YsNmV5"),
			const_address("Du4QkRu2rVwLcFBUJAGQ2DXPHTz6mVfNLNVyid5o6Vm6"),
			const_address("AZHLvwNcGdZP1AsGHFc2hzs11APJiGbEhkyK5VpyCBKa"),
			const_address("7hVJaSegGTdVtDwZ9iNJyPuSD3HX3iZ9SDdCsqShkypc"),
			const_address("8NwHCwPfzpyQvQxXTypmw4QQdHxLpZrmuyJ2wBRny2cE"),
			const_address("FQyP8Pe4xFaeu1wPEaA3nqor3UrtWdFMYTXq4J92JEoZ"),
			const_address("3B3Vwvfx1ZWwcrf1i5F26w4zs7SpMva4JZMnMob8FKvs"),
			const_address("FRB7dgrjcvvGc4faqhXQyzPwvNBacx7AQoGURiA721q9"),
			const_address("6jGyYPcu1QRfyV7s99QZ5DyaTzsjbvaDcyTNiYfF2j2k"),
			const_address("CcGQ73N19U5Po99FrcjLsCHLsSdvT276tCmesZckxzrc"),
			const_address("7zne7jv6cvTLBaTTMCFvvqXwpMdqwwSdWY58n2v7xXgY"),
			const_address("FfRe1ZrayiNd4uVrCg8CoWKHvZrdQZqGpSHT9BPMLq5N"),
			const_address("8xqgHheNm75KgfxXrwTH84vVCJFbRfgiDYikaXLcpEgv"),
			const_address("5DrhcUmXwoWLwzeCU3xVhAjg1MHL8JqcpAisX645NPSW"),
			const_address("98ENa65H4azGmaEdn3kx7VMmy5Hx73jZdShAbvQaaTy5"),
			const_address("B1LUePw4D7PwcFqNbbNBSYJjopgBQSV4NYKmEgqNAN5v"),
			const_address("AdKGe6Bv1qFUUzoLv9BQKRn49RCM7sVxrHVy5zniAznn"),
			const_address("BQPXeAXL89DcffrdfCpqNNcu5ehQdvHYZL75pLS1GMxg"),
			const_address("G5xssHyVV1r3bLRastAXnr27cvB3KYBMjwEDD5H4nqxU"),
			const_address("Gj5CfJA4nP6m5xHekk28QRAanJUJEFFx2fjKHUdSagGY"),
			const_address("G9dvMwe1hJuSGrnqLdkbSnuWH386iL3UuxYuJz64FeLf"),
			const_address("BuCN3zHPSfy1489ajoiVD3cNstpLMrePyeTs4QAcENyH"),
			const_address("2zMqwgU9xm4foAaHGnYKiWANePwb4bhfYREyU9HSK6Eb"),
		],
	),
	chainlink_oracle_price_settings: ChainlinkOraclePriceSettings {
		arb_address_checker: H160(hex_literal::hex!("564e411634189E68ecD570400eBCF783b4aF8688")),
		arb_oracle_feeds: Array {
			array: [
				H160(hex_literal::hex!("56a43EB56Da12C0dc1D972ACb089c06a5dEF8e69")),
				H160(hex_literal::hex!("d30e2101a97dcbAeBCBC04F14C3f624E67A35165")),
				H160(hex_literal::hex!("32377717BC9F9bA8Db45A244bCE77e7c0Cc5A775")),
				H160(hex_literal::hex!("0153002d20B96532C639313c2d54c3dA09109309")),
				H160(hex_literal::hex!("80EDee6f667eCc9f63a0a6f55578F870651f06A4")),
			],
		},
		eth_address_checker: H160(hex_literal::hex!("26061f315570bddf11d9055411a3d811c5ff0148")),
		eth_oracle_feeds: Array {
			array: [
				H160(hex_literal::hex!("1b44F3514812d835EB1BDB0acB33d3fA3351Ee43")),
				H160(hex_literal::hex!("694AA1769357215DE4FAC081bf1f309aDC325306")),
				// There is no SOL price feed in testnet - using ETH instead
				H160(hex_literal::hex!("694AA1769357215DE4FAC081bf1f309aDC325306")),
				H160(hex_literal::hex!("A2F78ab2355fe2f984D808B5CeE7FD0A93D5270E")),
				// There is no USDT price feed in testnet - using USDC instead
				H160(hex_literal::hex!("A2F78ab2355fe2f984D808B5CeE7FD0A93D5270E")),
			],
		},
	},
};

pub const BASHFUL_ACCOUNT_ID: &str = "cFLbasoV5juCGacy9LvvwSgkupFiFmwt8RmAuA3xcaY5YmkBe";
pub const BASHFUL_SR25519: [u8; 32] =
	hex_literal::hex!["789522255805797fd542969100ab7689453cd5697bb33619f5061e47b7c1564f"];
pub const BASHFUL_ED25519: [u8; 32] =
	hex_literal::hex!["e4f9260f8ed3bd978712e638c86f85a57f73f9aadd71538eea52f05dab0df2dd"];
pub const DOC_ACCOUNT_ID: &str = "cFLdocdoGZTwNpUZYDTNYTg6VHBEe5XscrzA8yUL36ZDXFeTw";
pub const DOC_SR25519: [u8; 32] =
	hex_literal::hex!["7a46817c60dff154901510e028f865300452a8d7a528f573398313287c689929"];
pub const DOC_ED25519: [u8; 32] =
	hex_literal::hex!["15bb6ba6d89ee9fac063dbf5712a4f53fa5b5a7b18e805308575f4732cb0061f"];
pub const DOPEY_ACCOUNT_ID: &str = "cFLdopTf8QEQbUErALYyZXvbCUzTCGWYMi9v9BZEGZbR9sGzv";
pub const DOPEY_SR25519: [u8; 32] =
	hex_literal::hex!["7a47312f9bd71d480b1e8f927fe8958af5f6345ac55cb89ef87cff5befcb0949"];
pub const DOPEY_ED25519: [u8; 32] =
	hex_literal::hex!["7c937c229aa95b19732a4a2e306a8cefb480e7c671de8fc416ec01bb3eedb749"];
pub const SNOW_WHITE_ACCOUNT_ID: &str = "cFLsnoVqoi2DdzewWg5NQDaQC2rLwjPeNJ5AGxEYRpw49wFir";
pub const SNOW_WHITE_SR25519: [u8; 32] =
	hex_literal::hex!["84f134a4cc6bf41d3239bbe097eac4c8f83e78b468e6c49ed5cd2ddc51a07a29"];

pub const EPOCH_DURATION_BLOCKS: BlockNumber = 3 * HOURS;

pub fn extra_accounts() -> Vec<(AccountId, AccountRole, FlipBalance, Option<Vec<u8>>)> {
	vec![
		(
			hex_literal::hex!("2efeb485320647a8d472503591f8fce9268cc3bf1bb8ad02efd2e905dcd1f31e")
				.into(),
			AccountRole::Broker,
			100 * FLIPPERINOS_PER_FLIP,
			Some(b"Chainflip Sisyphos Broker".to_vec()),
		),
		(
			hex_literal::hex!("c0409f949ad2636d34e4c70dd142296fdd4a11323d320aced3d247ad8f9a7902")
				.into(),
			AccountRole::LiquidityProvider,
			100 * FLIPPERINOS_PER_FLIP,
			Some(b"Chainflip Sisyphos LP".to_vec()),
		),
	]
}

pub const BITCOIN_SAFETY_MARGIN: u64 = 5;
pub const ETHEREUM_SAFETY_MARGIN: u64 = 2;
pub const ARBITRUM_SAFETY_MARGIN: u64 = 1;
pub const SOLANA_SAFETY_MARGIN: u64 = 1; // Unused - we use "finalized" instead
