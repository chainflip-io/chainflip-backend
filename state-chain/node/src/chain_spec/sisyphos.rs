use super::StateChainEnvironment;
pub use super::{
	common::*,
	testnet::{
		ARBITRUM_EXPIRY_BLOCKS, BITCOIN_EXPIRY_BLOCKS, ETHEREUM_EXPIRY_BLOCKS,
		POLKADOT_EXPIRY_BLOCKS, SOLANA_EXPIRY_BLOCKS,
	},
};
use cf_chains::{
	dot::RuntimeVersion,
	sol::{SolAddress, SolHash},
};
use cf_primitives::{AccountId, AccountRole, BlockNumber, FlipBalance, NetworkEnvironment};
use sc_service::ChainType;
use sp_core::H256;
use utilities::bs58_array;

use sol_prim::consts::{const_address, const_hash};

pub struct Config;

pub const NETWORK_NAME: &str = "Chainflip-Sisyphos";
pub const CHAIN_TYPE: ChainType = ChainType::Live;
pub const NETWORK_ENVIRONMENT: NetworkEnvironment = NetworkEnvironment::Testnet;
pub const PROTOCOL_ID: &str = "flip-sisy-2";

pub const GENESIS_FUNDING_AMOUNT: FlipBalance = 1_000 * FLIPPERINOS_PER_FLIP;

pub const ENV: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("cD079EAB6B5443b545788Fd210C8800FEADd87fa"),
	eth_usdc_address: hex_literal::hex!("1c7D4B196Cb0C7B01d743Fbc6116a902379C7238"),
	eth_usdt_address: hex_literal::hex!("27cea6eb8a21aae05eb29c91c5ca10592892f584"),
	state_chain_gateway_address: hex_literal::hex!("1F7fE41C798cc7b1D34BdC8de2dDDA4a4bE744D9"),
	eth_key_manager_address: hex_literal::hex!("22f5562e6859924Db082b8B248ea0C974f148a17"),
	eth_vault_address: hex_literal::hex!("a94d6b1853F3cb611Ed3cCb701b4fdA5a9DACe85"),
	eth_address_checker_address: hex_literal::hex!("638e16DD15588B81257eBe9676FA1a0175FDB70a"),
	arb_key_manager_address: hex_literal::hex!("7EA74208E2954a7294097C731434caD29c5094D8"),
	arb_vault_address: hex_literal::hex!("8155BdD48CD011e1118b51A1C82be020A3E5c2f2"),
	arbusdc_token_address: hex_literal::hex!("75faf114eafb1BDbe2F0316DF893fd58CE46AA4d"),
	arb_address_checker_address: hex_literal::hex!("2e78F26e9798EBDe7F2b19736De6Aae4d51d6d34"),
	ethereum_chain_id: cf_chains::eth::CHAIN_ID_SEPOLIA,
	arbitrum_chain_id: cf_chains::arb::CHAIN_ID_ARBITRUM_SEPOLIA,
	eth_init_agg_key: hex_literal::hex!(
		"025e790770ed8e79c08d68fa781b2848651f3e94ef8b1305a7fb6de782798735ad"
	),
	ethereum_deployment_block: 5429873u64,
	genesis_funding_amount: GENESIS_FUNDING_AMOUNT,
	min_funding: MIN_FUNDING,
	dot_genesis_hash: H256(hex_literal::hex!(
		"5a7ebe8e4d69752907aef5a79e1908e2ceadd7f91cbe1e424d80621f7916ea24"
	)),
	dot_vault_account_id: None,
	dot_runtime_version: RuntimeVersion { spec_version: 10000, transaction_version: 25 },
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
	],
	sol_swap_endpoint_program: SolAddress(bs58_array(
		"7G6TxoGDsgaZX3HaKkrKyy28tsdr7ZGmeeMbXpm8R5HZ",
	)),
	sol_swap_endpoint_program_data_account: SolAddress(bs58_array(
		"mYabVW1uMXpGqwgHUBQu4Fg6GT9EMYUzYaGYbi3zgT7",
	)),
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
pub const ETHEREUM_SAFETY_MARGIN: u64 = 6;
pub const ARBITRUM_SAFETY_MARGIN: u64 = 1;
pub const SOLANA_SAFETY_MARGIN: u64 = 1; //TODO: put correct value
