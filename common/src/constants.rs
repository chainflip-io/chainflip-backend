// ======= Keygen and signing =======

/// This determines the average expected block time that we are targeting.
/// Blocks will be produced at a minimum duration defined by `SLOT_DURATION`.
/// `SLOT_DURATION` is picked up by `pallet_timestamp` which is in turn picked
/// up by `pallet_aura` to implement `fn slot_duration()`.
///
/// Change this to adjust the block time.
pub const MILLISECONDS_PER_BLOCK: u64 = 6000;

const SECONDS_PER_BLOCK: u64 = MILLISECONDS_PER_BLOCK / 1000;

/// Maximum duration a ceremony stage can last
pub const MAX_STAGE_DURATION_SECONDS: u32 = 300;

/// The number of blocks to wait for a threshold signature ceremony to complete.
pub const THRESHOLD_SIGNATURE_CEREMONY_TIMEOUT_BLOCKS: u32 =
    (MAX_STAGE_DURATION_SECONDS * 5) / SECONDS_PER_BLOCK as u32;

/// The maximum number of blocks to wait for a keygen to complete.
pub const KEYGEN_CEREMONY_TIMEOUT_BLOCKS: u32 =
    (MAX_STAGE_DURATION_SECONDS * 9) / SECONDS_PER_BLOCK as u32;
