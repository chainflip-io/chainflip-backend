use cf_primitives::{Asset, AssetAmount, AuthorityCount};
use sp_runtime::{Percent, Permill};
pub use state_chain_runtime::constants::common::*;
use state_chain_runtime::{chainflip::Offence, BlockNumber, FlipBalance, SetSizeParameters};

pub const GENESIS_FUNDING_AMOUNT: FlipBalance = 5_000 * FLIPPERINOS_PER_FLIP;
pub const MIN_FUNDING: FlipBalance = 10 * FLIPPERINOS_PER_FLIP;
pub const REDEMPTION_TAX: FlipBalance = 5 * FLIPPERINOS_PER_FLIP;
pub const MIN_AUTHORITIES: AuthorityCount = 2;
pub const AUCTION_PARAMETERS: SetSizeParameters = SetSizeParameters {
	min_size: MIN_AUTHORITIES,
	max_size: MAX_AUTHORITIES,
	max_expansion: MAX_AUTHORITIES,
};

/// Percent of the epoch we are allowed to redeem
pub const REDEMPTION_PERIOD_AS_PERCENTAGE: u8 = 50;

// Consider the equation (1 + x/1_000_000_000)^n = 1 + inf/100
// where inf is the target yearly inflation (percent), n is the number of compundings that
// we do in a year and x is the inflation rate (Perbill) for each compunding time period.

// The following values are calculated by solving the above equation for x using n =
// (365*14400)/150 (since compunding is done every heartbeat which is every 150 blocks) and inf
// is taken as 0.1 percent for authority emissions and 0.02 percent for backup node emissions.
pub const CURRENT_AUTHORITY_EMISSION_INFLATION_PERBILL: u32 = 28;
pub const BACKUP_NODE_EMISSION_INFLATION_PERBILL: u32 = 6;
pub const SUPPLY_UPDATE_INTERVAL: u32 = 24 * HOURS;

// This is equivalent to one reputation point for every minute of online time.
pub const REPUTATION_PER_HEARTBEAT: i32 = 15;
pub const ACCRUAL_RATIO: (i32, u32) = (REPUTATION_PER_HEARTBEAT, HEARTBEAT_BLOCK_INTERVAL);

const REPUTATION_PENALTY_SMALL: i32 = REPUTATION_PER_HEARTBEAT; // 15 minutes to recover reputation
const REPUTATION_PENALTY_MEDIUM: i32 = REPUTATION_PER_HEARTBEAT * 4; // One hour to recover reputation
const REPUTATION_PENALTY_LARGE: i32 = REPUTATION_PER_HEARTBEAT * 8; // Two hours to recover reputation

/// The offences committable within the protocol and their respective reputation penalty and
/// suspension durations.
pub const PENALTIES: &[(Offence, (i32, BlockNumber))] = &[
	(Offence::MissedHeartbeat, (REPUTATION_PENALTY_SMALL, 0)),
	(Offence::ParticipateKeygenFailed, (REPUTATION_PENALTY_MEDIUM, HEARTBEAT_BLOCK_INTERVAL)),
	(Offence::ParticipateSigningFailed, (REPUTATION_PENALTY_MEDIUM, MINUTES / 2)),
	(Offence::MissedAuthorshipSlot, (REPUTATION_PENALTY_LARGE, HEARTBEAT_BLOCK_INTERVAL)),
	(Offence::FailedToBroadcastTransaction, (REPUTATION_PENALTY_MEDIUM, HEARTBEAT_BLOCK_INTERVAL)),
	(Offence::GrandpaEquivocation, (REPUTATION_PENALTY_LARGE, HEARTBEAT_BLOCK_INTERVAL * 5)),
];

pub const MINIMUM_SWAP_AMOUNTS: &[(Asset, AssetAmount)] = &[
	(Asset::Eth, 580_000_000_000_000u128), // 1usd worth of Eth = 0.00058 * 18 d.p
	(Asset::Flip, FLIPPERINOS_PER_FLIP),   // 1 Flip
	(Asset::Usdc, 1_000_000u128),          // USDC = 6 d.p
	(Asset::Dot, 2_000_000_000u128),       // 1 USD worth of DOT = 0.2 * 10 d.p
	(Asset::Btc, 390_000u128),             // 1 USD worth of BTC = 0.000039 * 10 d.p
];

/// Daily slashing rate 0.1% (of the bond) for offline authority
pub const DAILY_SLASHING_RATE: Permill = Permill::from_perthousand(1);

/// Redemption delay on testnets is 2 MINUTES.
/// We use a ttl of 1 hour to give enough of a buffer.
pub const REDEMPTION_TTL_SECS: u64 = 2 * 3600;

/// Determines the expiry duration for governance proposals.
pub const EXPIRY_SPAN_IN_SECONDS: u64 = 24 * 3600;

pub const AUCTION_BID_CUTOFF_PERCENTAGE: Percent = Percent::from_percent(10);
