// ======= Keygen and signing =======

/// Maximum duration a ceremony stage can last
pub const MAX_STAGE_DURATION_SECONDS: u32 = 300; // TODO Look at this value

/// The number of blocks to wait for a threshold signature ceremony to complete.
pub const THRESHOLD_SIGNATURE_CEREMONY_TIMEOUT_BLOCKS: u32 = (MAX_STAGE_DURATION_SECONDS * 5) / 6;

/// The maximum number of blocks to wait for a keygen to complete.
pub const KEYGEN_CEREMONY_TIMEOUT_BLOCKS: u32 = (MAX_STAGE_DURATION_SECONDS * 9) / 6;
