use crate::{Address, Digest};
use cf_utilities::bs58_array;

pub const SOLANA_SIGNATURE_LEN: usize = 64;
pub const SOLANA_ADDRESS_LEN: usize = 32;
pub const SOLANA_DIGEST_LEN: usize = 32;

// NB: this includes the bump-seed!!!
pub const SOLANA_PDA_MAX_SEEDS: u8 = 16;
pub const SOLANA_PDA_MAX_SEED_LEN: usize = 32;
pub const SOLANA_PDA_MARKER: &[u8; 21] = b"ProgramDerivedAddress";

pub const HASH_BYTES: usize = 32;

/// Maximum string length of a base58 encoded pubkey
pub const MAX_BASE58_LEN: usize = 44;

/// Bit mask that indicates whether a serialized message is versioned.
pub const MESSAGE_VERSION_PREFIX: u8 = 0x80;

pub const fn const_address(s: &'static str) -> Address {
	Address(bs58_array(s))
}

pub const fn const_hash(s: &'static str) -> Digest {
	Digest(bs58_array(s))
}

// Solana native programs
pub const SYSTEM_PROGRAM_ID: Address = const_address("11111111111111111111111111111111");
pub const TOKEN_PROGRAM_ID: Address = const_address("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
pub const ASSOCIATED_TOKEN_PROGRAM_ID: Address =
	const_address("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
pub const SYS_VAR_RECENT_BLOCKHASHES: Address =
	const_address("SysvarRecentB1ockHashes11111111111111111111");
pub const SYS_VAR_INSTRUCTIONS: Address =
	const_address("Sysvar1nstructions1111111111111111111111111");
pub const ADDRESS_LOOKUP_TABLE_PROGRAM_ID: Address =
	const_address("AddressLookupTab1e1111111111111111111111111");

pub const SYS_VAR_RENT: Address = const_address("SysvarRent111111111111111111111111111111111");
pub const SYS_VAR_CLOCK: Address = const_address("SysvarC1ock11111111111111111111111111111111");
pub const BPF_LOADER_UPGRADEABLE_ID: Address =
	const_address("BPFLoaderUpgradeab1e11111111111111111111111");
pub const COMPUTE_BUDGET_PROGRAM: Address =
	const_address("ComputeBudget111111111111111111111111111111");

pub const MAX_TRANSACTION_LENGTH: usize = 1_232usize;
pub const MAX_COMPUTE_UNITS_PER_TRANSACTION: u32 = 1_400_000u32;
pub const MICROLAMPORTS_PER_LAMPORT: u32 = 1_000_000u32;
pub const LAMPORTS_PER_SIGNATURE: u64 = 5000u64;
pub const TOKEN_ACCOUNT_RENT: u64 = 2039280u64;

pub const NONCE_ACCOUNT_LENGTH: u64 = 80u64;

pub const SOL_USDC_DECIMAL: u8 = 6u8;
pub const ACCOUNT_KEY_LENGTH_IN_TRANSACTION: usize = 32usize;
pub const ACCOUNT_REFERENCE_LENGTH_IN_TRANSACTION: usize = 1usize;

pub const MAX_CCM_USER_ALTS: u8 = 5u8; // TODO: Albert come up with a good number for this
