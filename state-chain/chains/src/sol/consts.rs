pub const SOLANA_SIGNATURE_SIZE: usize = 64;
pub const SOLANA_ADDRESS_SIZE: usize = 32;

// NB: this includes the bump-seed!!!
pub const SOLANA_PDA_MAX_SEEDS: u8 = 16;
pub const SOLANA_PDA_MAX_SEED_LEN: usize = 32;
pub const SOLANA_PDA_MARKER: &[u8; 21] = b"ProgramDerivedAddress";

pub const SYSTEM_PROGRAM_ID: &str = "11111111111111111111111111111111";
