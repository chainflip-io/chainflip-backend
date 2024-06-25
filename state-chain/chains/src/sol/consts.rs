use crate::sol::{SolAddress, SolHash};

pub const SOLANA_SIGNATURE_SIZE: usize = 64;
pub const SOLANA_ADDRESS_SIZE: usize = 32;

// NB: this includes the bump-seed!!!
pub const SOLANA_PDA_MAX_SEEDS: u8 = 16;
pub const SOLANA_PDA_MAX_SEED_LEN: usize = 32;
pub const SOLANA_PDA_MARKER: &[u8; 21] = b"ProgramDerivedAddress";

pub const fn const_address(s: &'static str) -> SolAddress {
	SolAddress(bs58::decode(s.as_bytes()).into_array_const_unwrap())
}

pub const fn const_hash(s: &'static str) -> SolHash {
	SolHash(bs58::decode(s.as_bytes()).into_array_const_unwrap())
}

// Solana native programs
pub const SYSTEM_PROGRAM_ID: SolAddress = const_address("11111111111111111111111111111111");
pub const TOKEN_PROGRAM_ID: SolAddress =
	const_address("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
pub const ASSOCIATED_TOKEN_PROGRAM_ID: SolAddress =
	const_address("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
pub const SYS_VAR_RECENT_BLOCKHASHES: SolAddress =
	const_address("SysvarRecentB1ockHashes11111111111111111111");
pub const SYS_VAR_INSTRUCTIONS: SolAddress =
	const_address("Sysvar1nstructions1111111111111111111111111");
pub const SYS_VAR_RENT: SolAddress = const_address("SysvarRent111111111111111111111111111111111");
pub const SYS_VAR_CLOCK: SolAddress = const_address("SysvarC1ock11111111111111111111111111111111");
pub const BPF_LOADER_UPGRADEABLE_ID: SolAddress =
	const_address("BPFLoaderUpgradeab1e11111111111111111111111");
pub const COMPUTE_BUDGET_PROGRAM: SolAddress =
	const_address("ComputeBudget111111111111111111111111111111");

pub const MAX_TRANSACTION_LENGTH: usize = 1_232;
pub const SOL_USDC_DECIMAL: u8 = 6u8;
