use cf_primitives::{Asset, AssetAmount, AuthorityCount};
pub use state_chain_runtime::constants::common::*;
use state_chain_runtime::{chainflip::Offence, BlockNumber, FlipBalance};

pub const GENESIS_FUNDING_AMOUNT: FlipBalance = 5_000 * FLIPPERINOS_PER_FLIP;
pub const MIN_FUNDING: FlipBalance = 10 * FLIPPERINOS_PER_FLIP;
pub const REDEMPTION_TAX: FlipBalance = 5 * FLIPPERINOS_PER_FLIP;
pub const MIN_AUTHORITIES: AuthorityCount = 2;

/// Percent of the epoch we are allowed to redeem
pub const REDEMPTION_PERIOD_AS_PERCENTAGE: u8 = 50;

/// Annual inflation set aside for current authorities in basis points
pub const CURRENT_AUTHORITY_EMISSION_INFLATION_BPS: u32 = 10;
/// Annual inflation set aside for backup nodes in basis points
pub const BACKUP_NODE_EMISSION_INFLATION_BPS: u32 = 2;
pub const SUPPLY_UPDATE_INTERVAL: u32 = 24 * HOURS;

// Number of online credits required to get `ACCRUAL_REPUTATION_POINTS` of reputation
const ACCRUAL_ONLINE_CREDITS: u32 = 2500;
// Number of reputation points received for having `ACCRUAL_ONLINE_CREDITS`
const ACCRUAL_REPUTATION_POINTS: i32 = 1;
pub const ACCRUAL_RATIO: (i32, u32) = (ACCRUAL_REPUTATION_POINTS, ACCRUAL_ONLINE_CREDITS);

/// The offences committable within the protocol and their respective reputation penalty and
/// suspension durations.
pub const PENALTIES: &[(Offence, (i32, BlockNumber))] = &[
	(Offence::ParticipateKeygenFailed, (15, HEARTBEAT_BLOCK_INTERVAL)),
	(Offence::ParticipateSigningFailed, (15, HEARTBEAT_BLOCK_INTERVAL)),
	(Offence::MissedAuthorshipSlot, (15, HEARTBEAT_BLOCK_INTERVAL)),
	(Offence::MissedHeartbeat, (15, HEARTBEAT_BLOCK_INTERVAL)),
	// We exclude them from the nomination pool of the next attempt,
	// so there is no need to suspend them further.
	(Offence::FailedToBroadcastTransaction, (10, 0)),
	(Offence::GrandpaEquivocation, (50, HEARTBEAT_BLOCK_INTERVAL * 5)),
];

pub const SWAP_TTL: BlockNumber = 2 * HOURS;
pub const MINIMUM_SWAP_AMOUNTS: &[(Asset, AssetAmount)] = &[
	(Asset::Eth, 580_000_000_000_000u128), // 1usd worth of Eth = 0.00058 * 18 d.p
	(Asset::Flip, FLIPPERINOS_PER_FLIP),   // 1 Flip
	(Asset::Usdc, 1_000_000u128),          // USDC = 6 d.p
	(Asset::Dot, 2_000_000_000u128),       // 1 USD worth of DOT = 0.2 * 10 d.p
	(Asset::Btc, 390_000u128),             // 1 USD worth of BTC = 0.000039 * 10 d.p
];
