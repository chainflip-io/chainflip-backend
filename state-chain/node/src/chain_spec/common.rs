pub use state_chain_runtime::constants::common::*;

pub const CLAIM_DELAY_BUFFER_SECS_DEFAULT: u64 = 40;
pub const CURRENT_AUTHORITY_EMISSION_INFLATION_PERBILL_DEFAULT: u32 = 28;
pub const BACKUP_NODE_EMISSION_INFLATION_PERBILL_DEFAULT: u32 = 6;
pub const EXPIRY_SPAN_IN_SECONDS_DEFAULT: u64 = 80000;
pub const ACCRUAL_RATIO_DEFAULT: (i32, u32) = (1, 2500);
/// Percent of the epoch we are allowed to claim
pub const PERCENT_OF_EPOCH_PERIOD_CLAIMABLE_DEFAULT: u8 = 50;
/// Default supply update interval is 24 hours.
pub const SUPPLY_UPDATE_INTERVAL_DEFAULT: u32 = 14_400;

/// Most Ethereum blocks are validated in around 12 seconds. This is a conservative
/// time, in case things go wrong.
pub const CONSERVATIVE_BLOCK_TIME_SECS: u64 = 20;

pub const CLAIM_DELAY_BUFFER_SECS: u64 = CONSERVATIVE_BLOCK_TIME_SECS * eth::BLOCK_SAFETY_MARGIN;

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
