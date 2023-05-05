use cf_primitives::AuthorityCount;
pub use state_chain_runtime::constants::common::*;
use state_chain_runtime::{chainflip::Offence, BlockNumber, FlipBalance};

pub const GENESIS_FUNDING_AMOUNT: FlipBalance = 5_000 * FLIPPERINOS_PER_FLIP;
pub const MIN_FUNDING: FlipBalance = 10 * FLIPPERINOS_PER_FLIP;
pub const ETH_PRIORITY_FEE_PERCENTILE: u8 = 50;
pub const MIN_AUTHORITIES: AuthorityCount = 2;

/// Percent of the epoch we are allowed to redeem
pub const PERCENT_OF_EPOCH_PERIOD_REDEEMABLE: u8 = 50;

/// Most Ethereum blocks are validated in around 12 seconds. This is a conservative
/// time, in case things go wrong.
pub const CONSERVATIVE_BLOCK_TIME_SECS: u64 = 20;

pub const REDEMPTION_DELAY_BUFFER_SECS: u64 =
	CONSERVATIVE_BLOCK_TIME_SECS * eth::BLOCK_SAFETY_MARGIN;

// Consider the equation (1 + x/1_000_000_000)^n = 1 + inf/100
// where inf is the target yearly inflation (percent), n is the number of compundings that
// we do in a year and x is the inflation rate (Perbill) for each compunding time period.

// The following values are calculated by solving the above equation for x using n =
// (365*14400)/150 (since compunding is done every heartbeat which is every 150 blocks) and inf
// is taken as 0.1 percent for authority emissions and 0.02 percent for backup node emissions.
pub const CURRENT_AUTHORITY_EMISSION_INFLATION_PERBILL: u32 = 28;
pub const BACKUP_NODE_EMISSION_INFLATION_PERBILL: u32 = 6;
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
