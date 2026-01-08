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
use super::{get_account_id_from_seed, StateChainEnvironment};
use cf_chains::{dot::RuntimeVersion, sol::SolAddress};
use cf_primitives::{
	AccountId, AccountRole, BlockNumber, ChainflipNetwork, FlipBalance, NetworkEnvironment,
};
use cf_utilities::bs58_array;
use pallet_cf_elections::generic_tools::Array;
use sc_service::ChainType;
use sol_prim::consts::{const_address, const_hash};
use sp_core::{sr25519, H160, H256};
use state_chain_runtime::chainflip::generic_elections::ChainlinkOraclePriceSettings;

pub struct Config;

pub const NETWORK_NAME: &str = "Chainflip-Testnet";
pub const CHAIN_TYPE: ChainType = ChainType::Development;
pub const NETWORK_ENVIRONMENT: NetworkEnvironment = NetworkEnvironment::Development;
pub const CHAINFLIP_NETWORK: ChainflipNetwork = ChainflipNetwork::Development;
pub const PROTOCOL_ID: &str = "flip-test";

// These represent approximately 2 hours on testnet block times
pub const BITCOIN_EXPIRY_BLOCKS: u32 = 2 * 60 * 60 / (10 * 60);
pub const ETHEREUM_EXPIRY_BLOCKS: u32 = 2 * 60 * 60 / 14;
pub const ARBITRUM_EXPIRY_BLOCKS: u32 = 2 * 60 * 60 * 4;
pub const POLKADOT_EXPIRY_BLOCKS: u32 = 2 * 60 * 60 / 6;
pub const SOLANA_EXPIRY_BLOCKS: u32 = 2 * 60 * 60 * 10 / 4;
pub const ASSETHUB_EXPIRY_BLOCKS: u32 = 2 * 60 * 60 / 12;

pub const ENV: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("Cf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9"),
	eth_usdc_address: hex_literal::hex!("a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
	eth_usdt_address: hex_literal::hex!("Dc64a140Aa3E981100a9becA4E685f962f0cF6C9"),
	eth_wbtc_address: hex_literal::hex!("B7f8BC63BbcaD18155201308C8f3540b07f84F5e"),
	state_chain_gateway_address: hex_literal::hex!("9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0"),
	eth_key_manager_address: hex_literal::hex!("5FbDB2315678afecb367f032d93F642f64180aa3"),
	eth_vault_address: hex_literal::hex!("e7f1725E7734CE288F8367e1Bb143E90bb3F0512"),
	arb_key_manager_address: hex_literal::hex!("5FbDB2315678afecb367f032d93F642f64180aa3"),
	arb_vault_address: hex_literal::hex!("e7f1725E7734CE288F8367e1Bb143E90bb3F0512"),
	arb_usdc_token_address: hex_literal::hex!("Cf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9"),
	arb_usdt_token_address: hex_literal::hex!("5FC8d32690cc91D4c39d9d3abcBD16989F875707"),
	eth_address_checker_address: hex_literal::hex!("e7f1725E7734CE288F8367e1Bb143E90bb3F0512"),
	eth_sc_utils_address: hex_literal::hex!("610178dA211FEF7D417bC0e6FeD39F05609AD788"),
	arb_address_checker_address: hex_literal::hex!("9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0"),
	ethereum_chain_id: cf_chains::eth::CHAIN_ID_SEPOLIA,
	arbitrum_chain_id: cf_chains::arb::CHAIN_ID_ARBITRUM_SEPOLIA,
	eth_init_agg_key: hex_literal::hex!(
		"02e61afd677cdfbec838c6f309deff0b2c6056f8a27f2c783b68bba6b30f667be6"
	),
	#[cfg(feature = "runtime-benchmarks")]
	// Set initial agg key for benchmarking API call building
	sol_init_agg_key: Some(const_address("7x7wY9yfXjRmusDEfPPCreU4bP49kmH4mqjYUXNAXJoM")),
	#[cfg(not(feature = "runtime-benchmarks"))]
	sol_init_agg_key: None,
	ethereum_deployment_block: 0u64,
	genesis_funding_amount: GENESIS_FUNDING_AMOUNT,
	min_funding: MIN_FUNDING,
	dot_genesis_hash: H256(hex_literal::hex!(
		"570085b3449b7d267277c3055b6197d59bfdf9c0ce74d31f2286e0f685c31872"
	)),
	dot_vault_account_id: None,
	dot_runtime_version: RuntimeVersion { spec_version: 10000, transaction_version: 25 },
	hub_genesis_hash: H256(hex_literal::hex!(
		"fb173ab171945afba89bc3f964560ce7d870c99ff385c1678622907b523db172"
	)),
	hub_vault_account_id: None,
	hub_runtime_version: RuntimeVersion { spec_version: 1003004, transaction_version: 15 },
	sol_genesis_hash: None,
	sol_vault_program: SolAddress(bs58_array("8inHGLHXegST3EPLcpisQe9D1hDT9r7DJjS395L3yuYf")),
	sol_vault_program_data_account: SolAddress(bs58_array(
		"BttvFNSRKrkHugwDP6SpnBejCKKskHowJif1HGgBtTfG",
	)),
	sol_usdc_token_mint_pubkey: SolAddress(bs58_array(
		"24PNhTaNtomHhoy3fTRaMhAFCRj4uHqhZEEoWrKDbR5p",
	)),
	sol_token_vault_pda_account: SolAddress(bs58_array(
		"7B13iu7bUbBX88eVBqTZkQqrErnTMazPmGLdE5RqdyKZ",
	)),
	sol_usdc_token_vault_ata: SolAddress(bs58_array(
		"9CGLwcPknpYs3atgwtjMX7RhgvBgaqK8wwCvXnmjEoL9",
	)),
	sol_durable_nonces_and_accounts: [
		(
			const_address("2cNMwUCF51djw2xAiiU54wz1WrU8uG4Q8Kp8nfEuwghw"),
			const_hash("ATtt4cicTHjhUoqAR1gazU6JdQGLKSNqn7BSvveWp14m"),
		),
		(
			const_address("HVG21SovGzMBJDB9AQNuWb6XYq4dDZ6yUwCbRUuFnYDo"),
			const_hash("CKgsJpX1zE4AMByWV1mH1DLHWfi92aBXEPLnTbU7gvcU"),
		),
		(
			const_address("HDYArziNzyuNMrK89igisLrXFe78ti8cvkcxfx4qdU2p"),
			const_hash("GTVTpZBaZiwbpW6eVEEvhXqC7RXkvjruuiKgxvqxiSAg"),
		),
		(
			const_address("HLPsNyxBqfq2tLE31v6RiViLp2dTXtJRgHgsWgNDRPs2"),
			const_hash("71sH3oVczEMZhNHkFbDNziNtQLJSmuoUtJZPZKCCiXBA"),
		),
		(
			const_address("GKMP63TqzbueWTrFYjRwMNkAyTHpQ54notRbAbMDmePM"),
			const_hash("9ccnmA4SE4TAasCjLwHHnzZQ8YcvtvmtesEsGuBMh4mg"),
		),
		(
			const_address("EpmHm2aSPsB5ZZcDjqDhQ86h1BV32GFCbGSMuC58Y2tn"),
			const_hash("5VHtSfveFNXyX1CbNNjcqPPsax331UN3PJg6qpM3Mph"),
		),
		(
			const_address("9yBZNMrLrtspj4M7bEf2X6tqbqHxD2vNETw8qSdvJHMa"),
			const_hash("AbNrs22FNQEL869JfCfaBGYniUhyCq57zkyD8Kdy5Qz9"),
		),
		(
			const_address("J9dT7asYJFGS68NdgDCYjzU2Wi8uBoBusSHN1Z6JLWna"),
			const_hash("G2dNcTTqMATa5AEp97Vwy6ESY2ceEgGwbZ5Av9emcuD5"),
		),
		(
			const_address("GUMpVpQFNYJvSbyTtUarZVL7UDUgErKzDTSVJhekUX55"),
			const_hash("C5XRoKQx9uRnz7XqQutqWmCxPRAfbVUMQWCE8WECbEp8"),
		),
		(
			const_address("AUiHYbzH7qLZSkb3u7nAqtvqC7e41sEzgWjBEvXrpfGv"),
			const_hash("5jStixd7ve8UMto72nnvyj3S76mV3673BvT1ejK9U1yA"),
		),
		(
			const_address("BN2vyodNYQQTrx3gtaDAL2UGGVtZwFeF5M8krE5aYYES"),
			const_hash("Ei26neh7hgBaG53pb9BUAJysCbgwWFzepcqbyia9GKTa"),
		),
		(
			const_address("Gwq9TAQCjbJtdnmtxQa3PbHFfbr6YTUBMDjEP9x2uXnH"),
			const_hash("EbWq3dgSjaa8pX3YeXVHopANoHCAsEkXrbALMjndbvr1"),
		),
		(
			const_address("3pGbKatko2ckoLEy139McfKiirNgy9brYxieNqFGdN1W"),
			const_hash("64hFuYc2RjeDLWAatbnbD3XCWgRBgbHLxCjfeNxx1G5e"),
		),
		(
			const_address("9Mcd8BTievK2yTvyiqG9Ft4HfDFf6mjGFBWMnCSRQP8S"),
			const_hash("GALf62D4Km2XEbHswmuibs9QYRHWpgnDrTT4mUZWHGqn"),
		),
		(
			const_address("AEZG74RoqM6sxf79eTizq5ShB4JTuCkMVwUgtnC8H94z"),
			const_hash("EzoN8wxWK9VWSV4dLWcpZCVhvztV5W5GCWKSak4b9C4b"),
		),
		(
			const_address("APLkgyCWi8DFAMF4KikjTu8YnUG1r7sMjVEfDiaBRZnS"),
			const_hash("54s775GHBp5rD4CHCjgtTgR5YbrRHRuXiskd4APXnPuj"),
		),
		(
			const_address("4ShNXTTHvpVt6bQdZTRdyW6yWXDzrPupdMuxajbEoGE4"),
			const_hash("8nyoiZ5zXQPDagxKHozWxk5zsMkMPpS5vU1KHcASc4tH"),
		),
		(
			const_address("FgZp6NJYWw15U51ynfXCfU9vq3eVgDDAHMSfJ8fFBZZ8"),
			const_hash("G4GMjYZR5rbwATGXt1F5EfwzJJh4xTtUvbBA7nQmStVt"),
		),
		(
			const_address("ENQ9Mmg87KFLX8ncXRPDBSd7jhKCtPBi8QzAh4rkREgP"),
			const_hash("E9aQyeBWF8pXJrp4aaa97RssiE8SinBP4sofxLuYGbWv"),
		),
		(
			const_address("Hhay1UwkzkFUgrGUYuiCvUwv7kErNzAcZnVRQ2fetT7K"),
			const_hash("FYKwioMchMvi8uMW8dkhsivxDhL9NSfeeRQnvtTpb8RT"),
		),
		(
			const_address("2fUVR42opcHgGLrY1eguDXLYfQPHQe9ReJNmRorVt9v8"),
			const_hash("3MebcPyKDoWdNjBDR5GiMqoxk6giKghdo7yTVDRDP9dH"),
		),
		(
			const_address("HfKr1wJASkW5UHs8yNWAqMeaYJdp8K2mdYwkbdVRdVrm"),
			const_hash("Fymdispqod8j9so1fA5w6QRUXSf7qZLVUtwXRN3inwaD"),
		),
		(
			const_address("DrpYkMpJWkpNqX9yYgQfc3uZrCVYobJ3RbTABcSkHJkM"),
			const_hash("ADYS5N8F3UtHqPwkbHMmQviwgj6pXUWY9MeHBFgbtNJT"),
		),
		(
			const_address("HCXc3o2go1Y2KhfnykLYXEvofLifXTb7GT13w4GsFmGw"),
			const_hash("CZp2E3hnb9qsgoeE1nDc14wwkig5TXEDhT75BY89grnx"),
		),
		(
			const_address("FFKYhae4HSnMmA6JJfe8NNtZeySA9yRWLaHzE2jqfhBr"),
			const_hash("FXBSFEozmiXCb7BoYMYPmthJTbRE9VULEbhvPfmpzarG"),
		),
		(
			const_address("AaRrJovR9Npna4fuCJ17AB3cJAMzoNDaZymRTbGGzUZm"),
			const_hash("CiwZV7WLhuDDRPFz372V1HadRJ8yioRk9TkPNftvhUe2"),
		),
		(
			const_address("5S8DzBBLvJUeyJccV4DekAK8KJA5PDcjwxRxCvgdyBEi"),
			const_hash("Cx7GTwXmBSERUDJbUSxTefmqfG36TVHgL1JEMPBPzpyZ"),
		),
		(
			const_address("Cot1DQZpm859brrre7swrDhTYLj2NJbg3hdMKCHk5zSk"),
			const_hash("D1R5p7S3zm23WxbJWUUP5WTrRjSaCs4T7QQL1Em2niTg"),
		),
		(
			const_address("4mfDv7PisvtMhiyGmvD6vxRdVpB842XbUhimAZYxMEn9"),
			const_hash("A1Vzqynswr4Hi57T6uuc9jKemhQB1Z2bPKpQTPwpU9rj"),
		),
		(
			const_address("BHW7qFCNHTX5QD5yJpT1hn1VM817Ji5ksZqiXMfqGrsj"),
			const_hash("13MfbQex4kW8eChdPhcbnZKDif8ZWpEvpxhUA61rLG6X"),
		),
		(
			const_address("EJqZLeaxi2gVsJgQW4nbmxyWJukK25n7jB8qWKoDgWUN"),
			const_hash("GbSUjgz6xGwpiusgCMRyoGdsp5B1U9txqCYmjKtKPumo"),
		),
		(
			const_address("BJqTPWyoqqgzhkLh1pbPh4KWBqg8kCUNzJ81avitSQrm"),
			const_hash("B2No3zYa3QFMbG1LkfDDqekXqmQWn4C5djvdzurJb6Bx"),
		),
		(
			const_address("EkmPmEmSbwm8EDDYtLtaDgcfuLNtW7MbKx5w3FUpaGjv"),
			const_hash("546nX7nm3PMdZwq9caYbR4VtMyU8UHHHS9PmzWyBJ1Z1"),
		),
		(
			const_address("CgwtCv8HQ67imnHEkz24TfXfyA2H5jurxcLGxAgDmNQj"),
			const_hash("6idmaUzkSZ5z8ovoVhzYLKjrhpSVrT7Bm8zs8eHroy4H"),
		),
		(
			const_address("zfKsXSxJ4cTpKS7S6aHL1Hy3m1CEjQuySKSwkWvukQX"),
			const_hash("GfqfzAtoWSP1cahryXVKQ7opER7RvM8Q3ELqFpvrnmQN"),
		),
		(
			const_address("2VvN1s6txNYyBdKpaC8b6AZKVqUQiQT2Exrpa7ffCgV6"),
			const_hash("2kahYJmdPjy1g6f566ZDMkhAumLDRukjiGPWoq7PMoa7"),
		),
		(
			const_address("A2DT1dc4rA1uMry7WCLwoUEQQNjCAsAMkB4X9Lgo88zd"),
			const_hash("FbqNU1MDXbcxb85zPD4CoRJcDx3Q3skkpzv7U9dSZPKp"),
		),
		(
			const_address("9mNBRGfTMLsSsQUn4YZfRDBVXfQ6juEWbNUTwv2ir9gC"),
			const_hash("9tbBPi74uZ2aWm33bpwvL5ge2bane6MVz8mLuiA79KEK"),
		),
		(
			const_address("3jXiydxPx1P7Ggdja5yt384ryLJAW2c8LRGV8PPRT54C"),
			const_hash("tuv4XtrVATYpfzmkS2enymAji48BfH9TXAFehww8Mow"),
		),
		(
			const_address("7ztGR1z28NpYjUaXyrGBzBGu62u1f9H9Pj9UVSKnT3yu"),
			const_hash("J3JTYrjMzCU3MnH2jEKrx3SqM7XWWNhLycHivqVjfbGN"),
		),
		(
			const_address("4GdnDTr5X4eJFHuzTEBLrz3tsREo8rQro7S9YDqrbMZ9"),
			const_hash("52TTBZfoc4pfbfHxmff5HeJms8P6JDeJseFiZmqpqxJs"),
		),
		(
			const_address("ALxnH6TBKJPBFRfFZspQkxDjb9nGLUP5oxFFdZNRFgUu"),
			const_hash("7UVTKi2dvGuRhCgedJ3UqmQ3qAx4JSiwDkPfJaAhZ2mp"),
		),
		(
			const_address("Bu3sdWtBh5TJishgK3vneh2zJg1rjLqWN5mFTHxWspwJ"),
			const_hash("FUyToeoynggaYxfkXnPcqWV2nw85B5TrV7QowEMwkPEc"),
		),
		(
			const_address("GvBbUTE312RXU5iXAcNWt6CuVbfsPs5Nk28D6qvU6NF3"),
			const_hash("GozUYbpAgoFkv5KKMQGe9jAzqU9YPzGbtEniDD3V9xZ8"),
		),
		(
			const_address("2LLct8SsnkW3sD9Gu8CfxmDEjKAWtFXqLvA8ymMyuq8u"),
			const_hash("3qRw7McbQBKazULB2HSuFSvxBZJERWBQbTvcQuyeCGqp"),
		),
		(
			const_address("CQ9vUhC3dSa4LyZCpWVpNbXhSn6f7J3NQXWDDvMMk6aW"),
			const_hash("BV7c7CUU7VXSBsWFJ71dVBFwGHevZDpb95cb6y4o7isM"),
		),
		(
			const_address("Cw8GqRmKzCbp7UFfafECC9sf9f936Chgx3BkbSgnXfmU"),
			const_hash("Yqr8Cg9XTkDiqdpTde3sNtdbsSJ3NxmfvHQJhmfmpCe"),
		),
		(
			const_address("GFJ6m6YdNT1tUfAxyD2BiPSx8gwt3xe4jVAKdtdSUt8W"),
			const_hash("CFQLVBG9Kh4uLNtegTuja6smbNCFDhQzh3utaoqmti7z"),
		),
		(
			const_address("7bphTuo5BKs4JJw5WPusCevmnoRk9ocFiB8EGgfwnh4c"),
			const_hash("Ctb9q1xjptQAQHvf8R9DgYw8NYrCzMwyGNtYKkZwiy8U"),
		),
		(
			const_address("EFbUq18Mcdi2gGauRzmbNeD5ixaB7EYVk5JZgAF34LoS"),
			const_hash("97q7MC5D7spBAaqP5aa4TBA25xdQGYBG7JxPc34auYzK"),
		),
	],
	sol_swap_endpoint_program: SolAddress(bs58_array(
		"35uYgHdfZQT4kHkaaXQ6ZdCkK5LFrsk43btTLbGCRCNT",
	)),
	sol_swap_endpoint_program_data_account: SolAddress(bs58_array(
		"2tmtGLQcBd11BMiE9B1tAkQXwmPNgR79Meki2Eme4Ec9",
	)),
	sol_alt_manager_program: SolAddress(bs58_array("49XegQyykAXwzigc6u7gXbaLjhKfNadWMZwFiovzjwUw")),
	sol_address_lookup_table_account: (
		SolAddress(bs58_array("8LXZqH1qKyciLmJ6eY7hjWj6KtuCw3FLqMeb7zxVWChT")),
		[
			const_address("BttvFNSRKrkHugwDP6SpnBejCKKskHowJif1HGgBtTfG"),
			const_address("SysvarRecentB1ockHashes11111111111111111111"),
			const_address("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"),
			const_address("7B13iu7bUbBX88eVBqTZkQqrErnTMazPmGLdE5RqdyKZ"),
			const_address("9CGLwcPknpYs3atgwtjMX7RhgvBgaqK8wwCvXnmjEoL9"),
			const_address("24PNhTaNtomHhoy3fTRaMhAFCRj4uHqhZEEoWrKDbR5p"),
			const_address("Sysvar1nstructions1111111111111111111111111"),
			const_address("2tmtGLQcBd11BMiE9B1tAkQXwmPNgR79Meki2Eme4Ec9"),
			const_address("EWaGcrFXhf9Zq8yxSdpAa75kZmDXkRxaP17sYiL6UpZN"),
			const_address("So11111111111111111111111111111111111111112"),
			const_address("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"),
			const_address("11111111111111111111111111111111"),
			const_address("8inHGLHXegST3EPLcpisQe9D1hDT9r7DJjS395L3yuYf"),
			const_address("35uYgHdfZQT4kHkaaXQ6ZdCkK5LFrsk43btTLbGCRCNT"),
			const_address("49XegQyykAXwzigc6u7gXbaLjhKfNadWMZwFiovzjwUw"),
			const_address("2cNMwUCF51djw2xAiiU54wz1WrU8uG4Q8Kp8nfEuwghw"),
			const_address("HVG21SovGzMBJDB9AQNuWb6XYq4dDZ6yUwCbRUuFnYDo"),
			const_address("HDYArziNzyuNMrK89igisLrXFe78ti8cvkcxfx4qdU2p"),
			const_address("HLPsNyxBqfq2tLE31v6RiViLp2dTXtJRgHgsWgNDRPs2"),
			const_address("GKMP63TqzbueWTrFYjRwMNkAyTHpQ54notRbAbMDmePM"),
			const_address("EpmHm2aSPsB5ZZcDjqDhQ86h1BV32GFCbGSMuC58Y2tn"),
			const_address("9yBZNMrLrtspj4M7bEf2X6tqbqHxD2vNETw8qSdvJHMa"),
			const_address("J9dT7asYJFGS68NdgDCYjzU2Wi8uBoBusSHN1Z6JLWna"),
			const_address("GUMpVpQFNYJvSbyTtUarZVL7UDUgErKzDTSVJhekUX55"),
			const_address("AUiHYbzH7qLZSkb3u7nAqtvqC7e41sEzgWjBEvXrpfGv"),
			const_address("BN2vyodNYQQTrx3gtaDAL2UGGVtZwFeF5M8krE5aYYES"),
			const_address("Gwq9TAQCjbJtdnmtxQa3PbHFfbr6YTUBMDjEP9x2uXnH"),
			const_address("3pGbKatko2ckoLEy139McfKiirNgy9brYxieNqFGdN1W"),
			const_address("9Mcd8BTievK2yTvyiqG9Ft4HfDFf6mjGFBWMnCSRQP8S"),
			const_address("AEZG74RoqM6sxf79eTizq5ShB4JTuCkMVwUgtnC8H94z"),
			const_address("APLkgyCWi8DFAMF4KikjTu8YnUG1r7sMjVEfDiaBRZnS"),
			const_address("4ShNXTTHvpVt6bQdZTRdyW6yWXDzrPupdMuxajbEoGE4"),
			const_address("FgZp6NJYWw15U51ynfXCfU9vq3eVgDDAHMSfJ8fFBZZ8"),
			const_address("ENQ9Mmg87KFLX8ncXRPDBSd7jhKCtPBi8QzAh4rkREgP"),
			const_address("Hhay1UwkzkFUgrGUYuiCvUwv7kErNzAcZnVRQ2fetT7K"),
			const_address("2fUVR42opcHgGLrY1eguDXLYfQPHQe9ReJNmRorVt9v8"),
			const_address("HfKr1wJASkW5UHs8yNWAqMeaYJdp8K2mdYwkbdVRdVrm"),
			const_address("DrpYkMpJWkpNqX9yYgQfc3uZrCVYobJ3RbTABcSkHJkM"),
			const_address("HCXc3o2go1Y2KhfnykLYXEvofLifXTb7GT13w4GsFmGw"),
			const_address("FFKYhae4HSnMmA6JJfe8NNtZeySA9yRWLaHzE2jqfhBr"),
			const_address("AaRrJovR9Npna4fuCJ17AB3cJAMzoNDaZymRTbGGzUZm"),
			const_address("5S8DzBBLvJUeyJccV4DekAK8KJA5PDcjwxRxCvgdyBEi"),
			const_address("Cot1DQZpm859brrre7swrDhTYLj2NJbg3hdMKCHk5zSk"),
			const_address("4mfDv7PisvtMhiyGmvD6vxRdVpB842XbUhimAZYxMEn9"),
			const_address("BHW7qFCNHTX5QD5yJpT1hn1VM817Ji5ksZqiXMfqGrsj"),
			const_address("EJqZLeaxi2gVsJgQW4nbmxyWJukK25n7jB8qWKoDgWUN"),
			const_address("BJqTPWyoqqgzhkLh1pbPh4KWBqg8kCUNzJ81avitSQrm"),
			const_address("EkmPmEmSbwm8EDDYtLtaDgcfuLNtW7MbKx5w3FUpaGjv"),
			const_address("CgwtCv8HQ67imnHEkz24TfXfyA2H5jurxcLGxAgDmNQj"),
			const_address("zfKsXSxJ4cTpKS7S6aHL1Hy3m1CEjQuySKSwkWvukQX"),
			const_address("2VvN1s6txNYyBdKpaC8b6AZKVqUQiQT2Exrpa7ffCgV6"),
			const_address("A2DT1dc4rA1uMry7WCLwoUEQQNjCAsAMkB4X9Lgo88zd"),
			const_address("9mNBRGfTMLsSsQUn4YZfRDBVXfQ6juEWbNUTwv2ir9gC"),
			const_address("3jXiydxPx1P7Ggdja5yt384ryLJAW2c8LRGV8PPRT54C"),
			const_address("7ztGR1z28NpYjUaXyrGBzBGu62u1f9H9Pj9UVSKnT3yu"),
			const_address("4GdnDTr5X4eJFHuzTEBLrz3tsREo8rQro7S9YDqrbMZ9"),
			const_address("ALxnH6TBKJPBFRfFZspQkxDjb9nGLUP5oxFFdZNRFgUu"),
			const_address("Bu3sdWtBh5TJishgK3vneh2zJg1rjLqWN5mFTHxWspwJ"),
			const_address("GvBbUTE312RXU5iXAcNWt6CuVbfsPs5Nk28D6qvU6NF3"),
			const_address("2LLct8SsnkW3sD9Gu8CfxmDEjKAWtFXqLvA8ymMyuq8u"),
			const_address("CQ9vUhC3dSa4LyZCpWVpNbXhSn6f7J3NQXWDDvMMk6aW"),
			const_address("Cw8GqRmKzCbp7UFfafECC9sf9f936Chgx3BkbSgnXfmU"),
			const_address("GFJ6m6YdNT1tUfAxyD2BiPSx8gwt3xe4jVAKdtdSUt8W"),
			const_address("7bphTuo5BKs4JJw5WPusCevmnoRk9ocFiB8EGgfwnh4c"),
			const_address("EFbUq18Mcdi2gGauRzmbNeD5ixaB7EYVk5JZgAF34LoS"),
		],
	),
	chainlink_oracle_price_settings: ChainlinkOraclePriceSettings {
		arb_address_checker: H160(hex_literal::hex!("9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0")),
		arb_oracle_feeds: Array {
			array: [
				H160(hex_literal::hex!("0165878A594ca255338adfa4d48449f69242Eb8F")),
				H160(hex_literal::hex!("a513E6E4b8f2a923D98304ec87F64353C4D5C853")),
				H160(hex_literal::hex!("2279B7A0a67DB372996a5FaB50D91eAA73d2eBe6")),
				H160(hex_literal::hex!("8A791620dd6260079BF849Dc5567aDC3F2FdC318")),
				H160(hex_literal::hex!("610178dA211FEF7D417bC0e6FeD39F05609AD788")),
			],
		},
		eth_address_checker: H160(hex_literal::hex!("e7f1725E7734CE288F8367e1Bb143E90bb3F0512")),
		eth_oracle_feeds: Array{ array:[
			H160(hex_literal::hex!("5FC8d32690cc91D4c39d9d3abcBD16989F875707")),
			H160(hex_literal::hex!("0165878A594ca255338adfa4d48449f69242Eb8F")),
			H160(hex_literal::hex!("a513E6E4b8f2a923D98304ec87F64353C4D5C853")),
			H160(hex_literal::hex!("2279B7A0a67DB372996a5FaB50D91eAA73d2eBe6")),
			H160(hex_literal::hex!("8A791620dd6260079BF849Dc5567aDC3F2FdC318")),
		]},
	},
};

pub const EPOCH_DURATION_BLOCKS: BlockNumber = 3 * HOURS;

pub const BASHFUL_ACCOUNT_ID: &str = "cFK7GTahm9qeX5Jjct3yfSvV4qLb8LJaArHL2SL6m9HAzc2sq";
pub const BASHFUL_SR25519: [u8; 32] =
	hex_literal::hex!["36c0078af3894b8202b541ece6c5d8fb4a091f7e5812b688e703549040473911"];
pub const BASHFUL_ED25519: [u8; 32] =
	hex_literal::hex!["971b584324592e9977f0ae407eb6b8a1aa5bcd1ca488e54ab49346566f060dd8"];
pub const DOC_ACCOUNT_ID: &str = "cFLxadYLtGwLKA4QZ7XM7KEtmwEohJJy4rVGCG6XK6qS1reye";
pub const DOC_SR25519: [u8; 32] =
	hex_literal::hex!["8898758bf88855615d459f552e36bfd14e8566c8b368f6a6448942759d5c7f04"];
pub const DOC_ED25519: [u8; 32] =
	hex_literal::hex!["e4c4009bd437cba06a2f25cf02f4efc0cac4525193a88fe1d29196e5d0ff54e8"];
pub const DOPEY_ACCOUNT_ID: &str = "cFNSnvbAqypZTfshHJxx9JLATURCvpQUFexn2bM1TaCZxnpbg";
pub const DOPEY_SR25519: [u8; 32] =
	hex_literal::hex!["ca58f2f4ae713dbb3b4db106640a3db150e38007940dfe29e6ebb870c4ccd47e"];
pub const DOPEY_ED25519: [u8; 32] =
	hex_literal::hex!["5506333c28f3dd39095696362194f69893bc24e3ec553dbff106cdcbfe1beea4"];
pub const SNOW_WHITE_ACCOUNT_ID: &str = "cFNYfLm7YEjWenMB7pBRGMTaawyhYLcRxgrNUqsvZBrKNXvfw";
pub const SNOW_WHITE_SR25519: [u8; 32] =
	hex_literal::hex!["ced2e4db6ce71779ac40ccec60bf670f38abbf9e27a718b4412060688a9ad212"];

pub fn extra_accounts() -> Vec<(AccountId, AccountRole, FlipBalance, Option<Vec<u8>>)> {
	vec![
		(
			get_account_id_from_seed::<sr25519::Public>("LP_API"),
			AccountRole::LiquidityProvider,
			100 * FLIPPERINOS_PER_FLIP,
			Some(b"Chainflip Testnet LP API".to_vec()),
		),
		(
			get_account_id_from_seed::<sr25519::Public>("LP_1"),
			AccountRole::LiquidityProvider,
			100 * FLIPPERINOS_PER_FLIP,
			Some(b"Chainflip Testnet LP 1".to_vec()),
		),
		(
			get_account_id_from_seed::<sr25519::Public>("LP_2"),
			AccountRole::LiquidityProvider,
			100 * FLIPPERINOS_PER_FLIP,
			Some(b"Chainflip Testnet LP 2".to_vec()),
		),
		(
			get_account_id_from_seed::<sr25519::Public>("LP_3"),
			AccountRole::LiquidityProvider,
			100 * FLIPPERINOS_PER_FLIP,
			Some(b"Chainflip Testnet LP 3".to_vec()),
		),
		(
			get_account_id_from_seed::<sr25519::Public>("LP_BOOST"),
			AccountRole::LiquidityProvider,
			100 * FLIPPERINOS_PER_FLIP,
			Some(b"Chainflip Testnet LP BOOST".to_vec()),
		),
		(
			get_account_id_from_seed::<sr25519::Public>("BROKER_1"),
			AccountRole::Broker,
			200 * FLIPPERINOS_PER_FLIP,
			Some(b"Chainflip Testnet Broker 1".to_vec()),
		),
		(
			get_account_id_from_seed::<sr25519::Public>("BROKER_2"),
			AccountRole::Broker,
			200 * FLIPPERINOS_PER_FLIP,
			Some(b"Chainflip Testnet Broker 2".to_vec()),
		),
		(
			get_account_id_from_seed::<sr25519::Public>("BROKER_FEE_TEST"),
			AccountRole::Broker,
			200 * FLIPPERINOS_PER_FLIP,
			Some(b"Chainflip Testnet Broker for broker_fee_collection_test".to_vec()),
		),
	]
}

pub const BITCOIN_SAFETY_MARGIN: u64 = 2;
pub const ETHEREUM_SAFETY_MARGIN: u64 = 2;
pub const ARBITRUM_SAFETY_MARGIN: u64 = 1;
pub const SOLANA_SAFETY_MARGIN: u64 = 1; //todo
