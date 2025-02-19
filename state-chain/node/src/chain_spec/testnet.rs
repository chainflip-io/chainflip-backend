pub use super::common::*;
use super::{get_account_id_from_seed, StateChainEnvironment};
use cf_chains::{dot::RuntimeVersion, sol::SolAddress};
use cf_primitives::{AccountId, AccountRole, BlockNumber, FlipBalance, NetworkEnvironment};
use cf_utilities::bs58_array;
use sc_service::ChainType;
use sol_prim::consts::{const_address, const_hash};
use sp_core::{sr25519, H256};

pub struct Config;

pub const NETWORK_NAME: &str = "Chainflip-Testnet";
pub const CHAIN_TYPE: ChainType = ChainType::Development;
pub const NETWORK_ENVIRONMENT: NetworkEnvironment = NetworkEnvironment::Development;
pub const PROTOCOL_ID: &str = "flip-test";

// These represent approximately 2 hours on testnet block times
pub const BITCOIN_EXPIRY_BLOCKS: u32 = 2 * 60 * 60 / (10 * 60);
pub const ETHEREUM_EXPIRY_BLOCKS: u32 = 2 * 60 * 60 / 14;
pub const ARBITRUM_EXPIRY_BLOCKS: u32 = 2 * 60 * 60 * 4;
pub const POLKADOT_EXPIRY_BLOCKS: u32 = 2 * 60 * 60 / 6;
pub const SOLANA_EXPIRY_BLOCKS: u32 = 2 * 60 * 60 * 10 / 4;

pub const ENV: StateChainEnvironment = StateChainEnvironment {
	flip_token_address: hex_literal::hex!("Cf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9"),
	eth_usdc_address: hex_literal::hex!("a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
	eth_usdt_address: hex_literal::hex!("0DCd1Bf9A1b36cE34237eEaFef220932846BCD82"),
	state_chain_gateway_address: hex_literal::hex!("9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0"),
	eth_key_manager_address: hex_literal::hex!("5FbDB2315678afecb367f032d93F642f64180aa3"),
	eth_vault_address: hex_literal::hex!("e7f1725E7734CE288F8367e1Bb143E90bb3F0512"),
	arb_key_manager_address: hex_literal::hex!("5FbDB2315678afecb367f032d93F642f64180aa3"),
	arb_vault_address: hex_literal::hex!("e7f1725E7734CE288F8367e1Bb143E90bb3F0512"),
	arbusdc_token_address: hex_literal::hex!("Cf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9"),
	eth_address_checker_address: hex_literal::hex!("e7f1725E7734CE288F8367e1Bb143E90bb3F0512"),
	arb_address_checker_address: hex_literal::hex!("9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0"),
	ethereum_chain_id: cf_chains::eth::CHAIN_ID_SEPOLIA,
	arbitrum_chain_id: cf_chains::arb::CHAIN_ID_ARBITRUM_SEPOLIA,
	eth_init_agg_key: hex_literal::hex!(
		"02e61afd677cdfbec838c6f309deff0b2c6056f8a27f2c783b68bba6b30f667be6"
	),
	ethereum_deployment_block: 0u64,
	genesis_funding_amount: GENESIS_FUNDING_AMOUNT,
	min_funding: MIN_FUNDING,
	dot_genesis_hash: H256(hex_literal::hex!(
		"13d0723c0891a46a0e0931e23fb7c9961c0f87bc73ad965b35cf0f1d84a986b8"
	)),
	dot_vault_account_id: None,
	dot_runtime_version: RuntimeVersion { spec_version: 10000, transaction_version: 25 },
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
			const_hash("A6PxZEnTwTrLQG8pVBwytG8YLRqPUeEdsXHJP2UQ5RSF"),
		),
		(
			const_address("HVG21SovGzMBJDB9AQNuWb6XYq4dDZ6yUwCbRUuFnYDo"),
			const_hash("EtK5bRt2pDX3CJyDzsdDtzjRf7v15NPR8JVpMikh6sNh"),
		),
		(
			const_address("HDYArziNzyuNMrK89igisLrXFe78ti8cvkcxfx4qdU2p"),
			const_hash("3PkYapCizvyiFPBEg2pkiBAxnVYfpR2VWs7H6FQcD7rc"),
		),
		(
			const_address("HLPsNyxBqfq2tLE31v6RiViLp2dTXtJRgHgsWgNDRPs2"),
			const_hash("HqPcXp31mYG1G4DP3c9F262pjXKeMzb4hbAwqsdLKrmM"),
		),
		(
			const_address("GKMP63TqzbueWTrFYjRwMNkAyTHpQ54notRbAbMDmePM"),
			const_hash("2cMqnuCCnGWm56LFPe9mZuGHdhzpFpwwv2io9Q99EMjE"),
		),
		(
			const_address("EpmHm2aSPsB5ZZcDjqDhQ86h1BV32GFCbGSMuC58Y2tn"),
			const_hash("7ZZpMge82HiNhhyv1LDzfwq7Ak9sF943TmLkQNuR7ZZh"),
		),
		(
			const_address("9yBZNMrLrtspj4M7bEf2X6tqbqHxD2vNETw8qSdvJHMa"),
			const_hash("Ee2tKBQguV5Rfsa738jBTRCU7vczXkZYnddiqwSRz2Dz"),
		),
		(
			const_address("J9dT7asYJFGS68NdgDCYjzU2Wi8uBoBusSHN1Z6JLWna"),
			const_hash("BhW9y8kkdBFiWnwzrYihhjhreovZd3TfZE7uaQnKz8ea"),
		),
		(
			const_address("GUMpVpQFNYJvSbyTtUarZVL7UDUgErKzDTSVJhekUX55"),
			const_hash("5CGa6yRJsVStdMR4PkUNGW5F13UeHBuqyurkmNByrgxj"),
		),
		(
			const_address("AUiHYbzH7qLZSkb3u7nAqtvqC7e41sEzgWjBEvXrpfGv"),
			const_hash("DCrChXBpKFjq61yYdULyYEnfqtcYkf1ACQqDNkgfwhF9"),
		),
		(
			const_address("BN2vyodNYQQTrx3gtaDAL2UGGVtZwFeF5M8krE5aYYES"),
			const_hash("4fjG6oYKadvnsbzAzomF5k2Zdc4DuuUyT71nueAeykMW"),
		),
		(
			const_address("Gwq9TAQCjbJtdnmtxQa3PbHFfbr6YTUBMDjEP9x2uXnH"),
			const_hash("GK29hbKjKWNwdF4KT11MzkrmQPsYPwE41qZMnLVcQPaS"),
		),
		(
			const_address("3pGbKatko2ckoLEy139McfKiirNgy9brYxieNqFGdN1W"),
			const_hash("5cinXdpw2KAGzmiXXegWJRdDDboXbDHaQaT3WFsH3txb"),
		),
		(
			const_address("9Mcd8BTievK2yTvyiqG9Ft4HfDFf6mjGFBWMnCSRQP8S"),
			const_hash("DRoAyPDtsg9CCMBSN6egFsWsP2zsQBAxCzN6fAdtQxJU"),
		),
		(
			const_address("AEZG74RoqM6sxf79eTizq5ShB4JTuCkMVwUgtnC8H94z"),
			const_hash("G8ZKHMsWFSoKJAtVbm1xgv8VjT5F6YBeiZbbzpVHuuyM"),
		),
		(
			const_address("APLkgyCWi8DFAMF4KikjTu8YnUG1r7sMjVEfDiaBRZnS"),
			const_hash("BMUqNXhMoB6VWsR7jHgRcv7yio8L5vjHdGby7gEJ4Pd2"),
		),
		(
			const_address("4ShNXTTHvpVt6bQdZTRdyW6yWXDzrPupdMuxajbEoGE4"),
			const_hash("52yamKJdMiQ5tEUyhkngvjR3XFXp7dmJzYsVsLbPs9JX"),
		),
		(
			const_address("FgZp6NJYWw15U51ynfXCfU9vq3eVgDDAHMSfJ8fFBZZ8"),
			const_hash("AX3qKNMBRKZimeCsBEhtp7heeduKekj85a4UpdN34HFe"),
		),
		(
			const_address("ENQ9Mmg87KFLX8ncXRPDBSd7jhKCtPBi8QzAh4rkREgP"),
			const_hash("GGFme2ydkkbDzq7LhVDMX5SsFf2yGLf7uKNSLLhvrGMd"),
		),
		(
			const_address("Hhay1UwkzkFUgrGUYuiCvUwv7kErNzAcZnVRQ2fetT7K"),
			const_hash("HMN14axscHALAuknuwSpVkEmAJkZWeJoNAjJuXUjRQbN"),
		),
		(
			const_address("2fUVR42opcHgGLrY1eguDXLYfQPHQe9ReJNmRorVt9v8"),
			const_hash("RWoH6shzxCS9dmW2mg37cujXxARBRBunbHEtZwUz1Gj"),
		),
		(
			const_address("HfKr1wJASkW5UHs8yNWAqMeaYJdp8K2mdYwkbdVRdVrm"),
			const_hash("2dhBBWYQE2Fvm4ShUQjno8ydJb5H6rUmBZ1e6TJHDupL"),
		),
		(
			const_address("DrpYkMpJWkpNqX9yYgQfc3uZrCVYobJ3RbTABcSkHJkM"),
			const_hash("6VCThFyLtFCKk35wihyrRUa6dubBU7skhJdRRDBfH4Md"),
		),
		(
			const_address("HCXc3o2go1Y2KhfnykLYXEvofLifXTb7GT13w4GsFmGw"),
			const_hash("EggsCqUiKJVxmN7a9s7RScXUAJRjekjwCAaeQvK9TcJ4"),
		),
		(
			const_address("FFKYhae4HSnMmA6JJfe8NNtZeySA9yRWLaHzE2jqfhBr"),
			const_hash("2E8BayMrqL3on6bhcMms6dsm3PwKcxttFSuHDNe6vZ4B"),
		),
		(
			const_address("AaRrJovR9Npna4fuCJ17AB3cJAMzoNDaZymRTbGGzUZm"),
			const_hash("D5bhT4vxhrtkfPeyZbvCtwzAnHSwBaa287CZZU8F6fye"),
		),
		(
			const_address("5S8DzBBLvJUeyJccV4DekAK8KJA5PDcjwxRxCvgdyBEi"),
			const_hash("8o7RkbTeV9r1yMgXaTzjcPys2FEkqieHER6Z5Fdc8hbw"),
		),
		(
			const_address("Cot1DQZpm859brrre7swrDhTYLj2NJbg3hdMKCHk5zSk"),
			const_hash("Gd86jHRKKSxrho3WKni5HYe6runTFX4cyFUQtcmJeiuk"),
		),
		(
			const_address("4mfDv7PisvtMhiyGmvD6vxRdVpB842XbUhimAZYxMEn9"),
			const_hash("4YQLN6N7G9nFwT8UVdFE2ZniW1gf89Qjob16wxMThxqN"),
		),
		(
			const_address("BHW7qFCNHTX5QD5yJpT1hn1VM817Ji5ksZqiXMfqGrsj"),
			const_hash("Ft3vnX4KQBa22CVpPkmvk5QNMGwL2xhQVxQtFJwK5MvJ"),
		),
		(
			const_address("EJqZLeaxi2gVsJgQW4nbmxyWJukK25n7jB8qWKoDgWUN"),
			const_hash("5tZeUYorHfdh9FYsA9yKjanFRwxjGxM9YLzNAfiwhZUf"),
		),
		(
			const_address("BJqTPWyoqqgzhkLh1pbPh4KWBqg8kCUNzJ81avitSQrm"),
			const_hash("5ggDcExaPfgjsmrhBS3D2UnRaEPsCGKGDkJqzF7gr92A"),
		),
		(
			const_address("EkmPmEmSbwm8EDDYtLtaDgcfuLNtW7MbKx5w3FUpaGjv"),
			const_hash("3G7D13ckfLCfDFC3VusXdittLHp6z6dZeUJBHTqcc2iR"),
		),
		(
			const_address("CgwtCv8HQ67imnHEkz24TfXfyA2H5jurxcLGxAgDmNQj"),
			const_hash("Gikpdfo6SwRqi3nTmQKuCmap3cGwupZd2poiYkUop4Sn"),
		),
		(
			const_address("zfKsXSxJ4cTpKS7S6aHL1Hy3m1CEjQuySKSwkWvukQX"),
			const_hash("43Kn8Kevfy2cy2fpJEhEZSpmPNwFurL2ERG5FqdSeTsq"),
		),
		(
			const_address("2VvN1s6txNYyBdKpaC8b6AZKVqUQiQT2Exrpa7ffCgV6"),
			const_hash("FVZKFoZ8WRdsFBp64LpTF1MrH36dHym2XZv7cJ2oYU5"),
		),
		(
			const_address("A2DT1dc4rA1uMry7WCLwoUEQQNjCAsAMkB4X9Lgo88zd"),
			const_hash("HjtHG8XRKyGiN8VXWmMh9oEviURAv5o2VygKTvsZjAPz"),
		),
		(
			const_address("9mNBRGfTMLsSsQUn4YZfRDBVXfQ6juEWbNUTwv2ir9gC"),
			const_hash("2S1aHds5wqUSyY4BmAK9YyBNGwzsQSsqGa2iyisF8t6h"),
		),
		(
			const_address("3jXiydxPx1P7Ggdja5yt384ryLJAW2c8LRGV8PPRT54C"),
			const_hash("Hgu6wkD6aE3uuESdW9fCWoXX4sN3eLnxYJsM7QCtrZMk"),
		),
		(
			const_address("7ztGR1z28NpYjUaXyrGBzBGu62u1f9H9Pj9UVSKnT3yu"),
			const_hash("9wic99ejEMQubHea9KKZZk27EU7r4LL8672D5GNrpXRG"),
		),
		(
			const_address("4GdnDTr5X4eJFHuzTEBLrz3tsREo8rQro7S9YDqrbMZ9"),
			const_hash("FCsgGf33ueodTVhLgQtTriNL5ZGfoaWoBkDwXSbDmLFd"),
		),
		(
			const_address("ALxnH6TBKJPBFRfFZspQkxDjb9nGLUP5oxFFdZNRFgUu"),
			const_hash("QkBgGAKFPtQzRj1v7sFECr8D2LMPb99AEy3w1RtKX5j"),
		),
		(
			const_address("Bu3sdWtBh5TJishgK3vneh2zJg1rjLqWN5mFTHxWspwJ"),
			const_hash("GWvckQW231Safveswww1GBSu4SzP5h5SgE6gugBn8upC"),
		),
		(
			const_address("GvBbUTE312RXU5iXAcNWt6CuVbfsPs5Nk28D6qvU6NF3"),
			const_hash("BnFsZdujQde7FnHGVcZTmvidRHBr5H87XRDDB6A5dn8D"),
		),
		(
			const_address("2LLct8SsnkW3sD9Gu8CfxmDEjKAWtFXqLvA8ymMyuq8u"),
			const_hash("Bnt1CWn8SEwpNqFbNxby6ysoW49wmL95Ed28pbS9v4Nx"),
		),
		(
			const_address("CQ9vUhC3dSa4LyZCpWVpNbXhSn6f7J3NQXWDDvMMk6aW"),
			const_hash("2yVSXvwXjtA5tqqWUKjxBuYjME6pKwJGA12NUc31x9VS"),
		),
		(
			const_address("Cw8GqRmKzCbp7UFfafECC9sf9f936Chgx3BkbSgnXfmU"),
			const_hash("FDPW3e2qPvNrmi1dqxsMaYAXLq9vMQYda5fKsVzNBUCv"),
		),
		(
			const_address("GFJ6m6YdNT1tUfAxyD2BiPSx8gwt3xe4jVAKdtdSUt8W"),
			const_hash("4tUTcUePrDUu48ZyH584aYv8JAbPrc9aDcH6bjxhEJon"),
		),
		(
			const_address("7bphTuo5BKs4JJw5WPusCevmnoRk9ocFiB8EGgfwnh4c"),
			const_hash("SyhFE8iYH9ZsZNBDWLvTDTBFBoEjxs12msF3xprikgf"),
		),
		(
			const_address("EFbUq18Mcdi2gGauRzmbNeD5ixaB7EYVk5JZgAF34LoS"),
			const_hash("53EBQYjZ7yH3Zy6KCWjSGAUGTki2YjxsHkrfXnvsj9vT"),
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
		SolAddress(bs58_array("7drVSq2ymJLNnXyCciHbNqHyzuSt1SL4iQSEThiESN2c")),
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
pub const ETHEREUM_SAFETY_MARGIN: u64 = 6;
pub const ARBITRUM_SAFETY_MARGIN: u64 = 1;
pub const SOLANA_SAFETY_MARGIN: u64 = 1; //todo
