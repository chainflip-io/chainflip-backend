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

pub use super::{
	common::*,
	testnet::{
		ARBITRUM_EXPIRY_BLOCKS, BITCOIN_EXPIRY_BLOCKS, ETHEREUM_EXPIRY_BLOCKS,
		POLKADOT_EXPIRY_BLOCKS, SOLANA_EXPIRY_BLOCKS,
	},
};
use super::{parse_account, StateChainEnvironment};
use cf_chains::{
	dot::RuntimeVersion,
	sol::{SolAddress, SolHash},
};
use cf_primitives::{AccountId, AccountRole, BlockNumber, FlipBalance, NetworkEnvironment};
use cf_utilities::bs58_array;
use sc_service::ChainType;
use sol_prim::consts::{const_address, const_hash};
use sp_core::H256;

pub struct Config;

pub const NETWORK_NAME: &str = "Chainflip-Perseverance";
pub const CHAIN_TYPE: ChainType = ChainType::Live;
pub const NETWORK_ENVIRONMENT: NetworkEnvironment = NetworkEnvironment::Testnet;
pub const PROTOCOL_ID: &str = "flip-pers-2";

pub const GENESIS_FUNDING_AMOUNT: FlipBalance = 1_000 * FLIPPERINOS_PER_FLIP;

pub const ENV: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("dC27c60956cB065D19F08bb69a707E37b36d8086"),
	eth_usdc_address: hex_literal::hex!("1c7D4B196Cb0C7B01d743Fbc6116a902379C7238"),
	eth_usdt_address: hex_literal::hex!("27cea6eb8a21aae05eb29c91c5ca10592892f584"),
	state_chain_gateway_address: hex_literal::hex!("A34a967197Ee90BB7fb28e928388a573c5CFd099"),
	eth_key_manager_address: hex_literal::hex!("4981b1329F29E720642266fc6e172C3f78159dff"),
	eth_vault_address: hex_literal::hex!("36eaD71325604DC15d35FAE584D7b50646D81753"),
	eth_address_checker_address: hex_literal::hex!("58eaCD5A40EEbCbBCb660f178F9A46B1Ad63F846"),
	arb_key_manager_address: hex_literal::hex!("18195b0E3c33EeF3cA6423b1828E0FE0C03F32Fd"),
	arb_vault_address: hex_literal::hex!("2bb150e6d4366A1BDBC4275D1F35892CD63F27e3"),
	arbusdc_token_address: hex_literal::hex!("75faf114eafb1BDbe2F0316DF893fd58CE46AA4d"),
	arb_address_checker_address: hex_literal::hex!("4F358eC5BD58c994f74B317554D7472769a0Ccf8"),
	ethereum_chain_id: cf_chains::eth::CHAIN_ID_SEPOLIA,
	arbitrum_chain_id: cf_chains::arb::CHAIN_ID_ARBITRUM_SEPOLIA,
	eth_init_agg_key: hex_literal::hex!(
		"021cf3c105fbc7112f3394c3e176463ec59600f1e7005ad8d68f66840264998667"
	),
	ethereum_deployment_block: 5429883u64,
	genesis_funding_amount: GENESIS_FUNDING_AMOUNT,
	min_funding: MIN_FUNDING,
	dot_genesis_hash: H256(hex_literal::hex!(
		"e566d149729892a803c3c4b1e652f09445926234d956a0f166be4d4dea91f536"
	)),
	dot_vault_account_id: None,
	dot_runtime_version: RuntimeVersion { spec_version: 10000, transaction_version: 25 },
	sol_genesis_hash: Some(SolHash(bs58_array("EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG"))),
	sol_vault_program: SolAddress(bs58_array("7ThGuS6a4KmX2rMFhqeCPHrRmmYEF7XoimGG53171xJa")),
	sol_vault_program_data_account: SolAddress(bs58_array(
		"GpTqSHz4JzQimjfDiBgDhJzYcTonj3t6kMhKTigCKHfc",
	)),
	sol_usdc_token_mint_pubkey: SolAddress(bs58_array(
		"4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU",
	)),
	sol_token_vault_pda_account: SolAddress(bs58_array(
		"2Uv7dCnuxuvyFnTRCyEyQpvwyYBhgFkWDm3b5Qdz9Agd",
	)),
	sol_usdc_token_vault_ata: SolAddress(bs58_array(
		"FYQrMSUQx3jrJMpu21mR8qzhpLXfa1nn65ZVqp4QSdEa",
	)),
	sol_durable_nonces_and_accounts: [
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
		// TODO: Update with the real nonces even if we'll actually insert
		// then via migration
		(
			const_address("11111111111111111111111111111111"),
			const_hash("4DEDrSVk4FRKFQkU1p9Zywi5MK64AGxC16RQZvhyFngq"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("5s1V7bXByHPquC1AYD94w4f8SgEhDjEnGeBtiPsuzXYU"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("7Y1PvrW65rZp3RqmJksix3XxQ9MrFdQ62NNbhdB97qwc"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("F1fe16vREumonQurbFAfmbKytfEE9khjy9UPjjgbGnG9"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("D6osW2CyHmpLqg8ymDAeNEjZZETqHGWdQBekh3cVjAUQ"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("7qDGqASPR3VannmDosTXUVMd5ZWbqDnawCA3auEHsq6r"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("4TFDiBqjU5istaaAovdgKBNDKJFdZ318W6XuC9MZiDBC"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("7ua7UjY1Csouw7K1nMDyWhL7Lx5PE9ernETcKciWALFH"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("742EN3zJUt6Xcrs1KAH4jfyLLVp8BYV2bSmjEpsFpMFo"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("FS1PdTqsDSEa9xUrLAS5k471MQsT28H2FW5CpUHiTmGF"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("4DEDrSVk4FRKFQkU1p9Zywi5MK64AGxC16RQZvhyFngq"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("5s1V7bXByHPquC1AYD94w4f8SgEhDjEnGeBtiPsuzXYU"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("7Y1PvrW65rZp3RqmJksix3XxQ9MrFdQ62NNbhdB97qwc"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("F1fe16vREumonQurbFAfmbKytfEE9khjy9UPjjgbGnG9"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("D6osW2CyHmpLqg8ymDAeNEjZZETqHGWdQBekh3cVjAUQ"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("7qDGqASPR3VannmDosTXUVMd5ZWbqDnawCA3auEHsq6r"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("4TFDiBqjU5istaaAovdgKBNDKJFdZ318W6XuC9MZiDBC"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("7ua7UjY1Csouw7K1nMDyWhL7Lx5PE9ernETcKciWALFH"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("742EN3zJUt6Xcrs1KAH4jfyLLVp8BYV2bSmjEpsFpMFo"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("FS1PdTqsDSEa9xUrLAS5k471MQsT28H2FW5CpUHiTmGF"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("4DEDrSVk4FRKFQkU1p9Zywi5MK64AGxC16RQZvhyFngq"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("5s1V7bXByHPquC1AYD94w4f8SgEhDjEnGeBtiPsuzXYU"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("7Y1PvrW65rZp3RqmJksix3XxQ9MrFdQ62NNbhdB97qwc"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("F1fe16vREumonQurbFAfmbKytfEE9khjy9UPjjgbGnG9"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("D6osW2CyHmpLqg8ymDAeNEjZZETqHGWdQBekh3cVjAUQ"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("7qDGqASPR3VannmDosTXUVMd5ZWbqDnawCA3auEHsq6r"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("4TFDiBqjU5istaaAovdgKBNDKJFdZ318W6XuC9MZiDBC"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("7ua7UjY1Csouw7K1nMDyWhL7Lx5PE9ernETcKciWALFH"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("742EN3zJUt6Xcrs1KAH4jfyLLVp8BYV2bSmjEpsFpMFo"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("FS1PdTqsDSEa9xUrLAS5k471MQsT28H2FW5CpUHiTmGF"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("4DEDrSVk4FRKFQkU1p9Zywi5MK64AGxC16RQZvhyFngq"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("5s1V7bXByHPquC1AYD94w4f8SgEhDjEnGeBtiPsuzXYU"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("7Y1PvrW65rZp3RqmJksix3XxQ9MrFdQ62NNbhdB97qwc"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("F1fe16vREumonQurbFAfmbKytfEE9khjy9UPjjgbGnG9"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("D6osW2CyHmpLqg8ymDAeNEjZZETqHGWdQBekh3cVjAUQ"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("7qDGqASPR3VannmDosTXUVMd5ZWbqDnawCA3auEHsq6r"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("4TFDiBqjU5istaaAovdgKBNDKJFdZ318W6XuC9MZiDBC"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("7ua7UjY1Csouw7K1nMDyWhL7Lx5PE9ernETcKciWALFH"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("742EN3zJUt6Xcrs1KAH4jfyLLVp8BYV2bSmjEpsFpMFo"),
		),
		(
			const_address("11111111111111111111111111111111"),
			const_hash("FS1PdTqsDSEa9xUrLAS5k471MQsT28H2FW5CpUHiTmGF"),
		),
	],
	sol_swap_endpoint_program: SolAddress(bs58_array(
		"DeL6iGV5RWrWh7cPoEa7tRHM8XURAaB4vPjfX5qVyuWE",
	)),
	sol_swap_endpoint_program_data_account: SolAddress(bs58_array(
		"12MYcNumSQCn81yKRfrk5P5ThM5ivkLiZda979hhKJDR",
	)),
	// TODO: To update with the right values
	sol_alt_manager_program: SolAddress(bs58_array("11111111111111111111111111111111")),
	sol_address_lookup_table_account: (
		SolAddress(bs58_array("11111111111111111111111111111111")),
		[
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
			const_address("11111111111111111111111111111111"),
		],
	),
};

pub const EPOCH_DURATION_BLOCKS: BlockNumber = 24 * HOURS;

pub const BASHFUL_ACCOUNT_ID: &str = "cFLbassb4hwQ9iA7dzdVdyumRqkaXnkdYECrThhmrqjFukdVo";
pub const BASHFUL_SR25519: [u8; 32] =
	hex_literal::hex!["789523326e5f007f7643f14fa9e6bcfaaff9dd217e7e7a384648a46398245d55"];
pub const BASHFUL_ED25519: [u8; 32] =
	hex_literal::hex!["7fdaaa9becf88f9f0a3590bd087ddce9f8d284ccf914c542e4c9f0c0e6440a6a"];
pub const DOC_ACCOUNT_ID: &str = "cFLdocJo3bjT7JbT7R46cA89QfvoitrKr9P3TsMcdkVWeeVLa";
pub const DOC_SR25519: [u8; 32] =
	hex_literal::hex!["7a467c9e1722b35408618a0cffc87c1e8433798e9c5a79339a10d71ede9e9d79"];
pub const DOC_ED25519: [u8; 32] =
	hex_literal::hex!["3489d0b548c5de56c1f3bd679dbabe3b0bff44fb5e7a377931c1c54590de5de6"];
pub const DOPEY_ACCOUNT_ID: &str = "cFLdopvNB7LaiBbJoNdNC26e9Gc1FNJKFtvNZjAmXAAVnzCk4";
pub const DOPEY_SR25519: [u8; 32] =
	hex_literal::hex!["7a4738071f16c71ef3e5d94504d472fdf73228cb6a36e744e0caaf13555c3c01"];
pub const DOPEY_ED25519: [u8; 32] =
	hex_literal::hex!["d9a7e774a58c50062caf081a69556736e62eb0c854461f4485f281f60c53160f"];
pub const SNOW_WHITE_ACCOUNT_ID: &str = "cFLsnoJA2YhAGt9815jPqmzK5esKVyhNAwPoeFmD3PEceE12a";
pub const SNOW_WHITE_SR25519: [u8; 32] =
	hex_literal::hex!["84f131a66e88e3e5f8dce20d413cab3fbb13769a14a4c7b640b7222863ef353d"];

pub fn extra_accounts() -> Vec<(AccountId, AccountRole, FlipBalance, Option<Vec<u8>>)> {
	[vec![
		(
			parse_account("cFMTNSQQVfBo2HqtekvhLPfZY764kuJDVFG1EvnnDGYxc3LRW"),
			AccountRole::Broker,
			1_000 * FLIPPERINOS_PER_FLIP,
			Some(b"Chainflip Genesis Broker".to_vec()),
		),
		(
			parse_account("cFN2sr3eDPoyp3G4CAg3EBRMo2VMoYJ7x3rBn51tmXsguYzMX"),
			AccountRole::LiquidityProvider,
			1_000 * FLIPPERINOS_PER_FLIP,
			Some(b"Chainflip Genesis Liquidity Provider".to_vec()),
		),
	]]
	.into_iter()
	.flatten()
	.collect()
}

pub const BITCOIN_SAFETY_MARGIN: u64 = 5;
pub const ETHEREUM_SAFETY_MARGIN: u64 = 6;
pub const ARBITRUM_SAFETY_MARGIN: u64 = 1;
pub const SOLANA_SAFETY_MARGIN: u64 = 1; // Unused - we use "finalized" instead
