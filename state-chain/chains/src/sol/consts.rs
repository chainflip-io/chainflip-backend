pub const SOLANA_SIGNATURE_SIZE: usize = 64;
pub const SOLANA_ADDRESS_SIZE: usize = 32;

// NB: this includes the bump-seed!!!
pub const SOLANA_PDA_MAX_SEEDS: u8 = 16;
pub const SOLANA_PDA_MAX_SEED_LEN: usize = 32;
pub const SOLANA_PDA_MARKER: &[u8; 21] = b"ProgramDerivedAddress";

// Solana native programs
pub const SYSTEM_PROGRAM_ID: &str = "11111111111111111111111111111111";
pub const TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const ASSOCIATED_TOKEN_PROGRAM_ID: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";
pub const SYS_VAR_RECENT_BLOCKHASHES: &str = "SysvarRecentB1ockHashes11111111111111111111";
pub const SYS_VAR_INSTRUCTIONS: &str = "Sysvar1nstructions1111111111111111111111111";
pub const SYS_VAR_RENT: &str = "SysvarRent111111111111111111111111111111111";
pub const SYS_VAR_CLOCK: &str = "SysvarC1ock11111111111111111111111111111111";
pub const BPF_LOADER_UPGRADEABLE_ID: &str = "BPFLoaderUpgradeab1e11111111111111111111111";
pub const COMPUTE_BUDGET_PROGRAM: &str = "ComputeBudget111111111111111111111111111111";

pub const MAX_TRANSACTION_LENGTH: usize = 1_232;
pub const SOL_USDC_DECIMAL: u8 = 6u8;
