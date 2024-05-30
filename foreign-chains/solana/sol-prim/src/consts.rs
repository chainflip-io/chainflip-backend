pub const SOLANA_ADDRESS_LEN: usize = 32;
pub const SOLANA_DIGEST_LEN: usize = 32;
pub const SOLANA_SIGNATURE_LEN: usize = 64;

// NB: this includes the bump-seed!!!
pub const SOLANA_PDA_MAX_SEEDS: u8 = 16;
pub const SOLANA_PDA_MAX_SEED_LEN: usize = 32;
pub const SOLANA_PDA_MARKER: &[u8; 21] = b"ProgramDerivedAddress";
