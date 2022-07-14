pub mod common {
	use cf_traits::{AuthorityCount, BlockNumber, FlipBalance};
	use pallet_cf_broadcast::AttemptCount;

	pub const CHAINFLIP_SS58_PREFIX: u16 = 2112;

	pub const TOTAL_ISSUANCE: FlipBalance = {
		const TOKEN_ISSUANCE: FlipBalance = 90_000_000;
		const TOKEN_DECIMALS: u32 = 18;
		const TOKEN_FRACTIONS: FlipBalance = 10u128.pow(TOKEN_DECIMALS);
		TOKEN_ISSUANCE * TOKEN_FRACTIONS
	};

	pub const MAX_AUTHORITIES: AuthorityCount = 150;

	// Number of online credits required to get `ACCRUAL_REPUTATION_POINTS` of reputation
	pub const ACCRUAL_ONLINE_CREDITS: u32 = 2500;
	// Number of reputation points received for having `ACCRUAL_ONLINE_CREDITS`
	pub const ACCRUAL_REPUTATION_POINTS: i32 = 1;
	pub const ACCRUAL_RATIO: (i32, u32) = (ACCRUAL_REPUTATION_POINTS, ACCRUAL_ONLINE_CREDITS);

	/// This determines the average expected block time that we are targeting.
	/// Blocks will be produced at a minimum duration defined by `SLOT_DURATION`.
	/// `SLOT_DURATION` is picked up by `pallet_timestamp` which is in turn picked
	/// up by `pallet_aura` to implement `fn slot_duration()`.
	///
	/// Change this to adjust the block time.
	pub const MILLISECONDS_PER_BLOCK: u64 = 6000;

	const SECONDS_PER_BLOCK: u64 = MILLISECONDS_PER_BLOCK / 1000;

	// ======= Keygen and signing =======

	/// Maximum duration a ceremony stage can last
	pub const MAX_STAGE_DURATION_SECONDS: u32 = 300;

	// Allow for the CFE to receive the finalised block (~3.5*6) for initiation, and some extra time
	// (~9) for networking / other latency
	const TIMEOUT_BUFFER_SECONDS: u32 = 30;

	const NUM_THRESHOLD_SIGNING_STAGES: u32 = 4;

	const NUM_KEYGEN_STAGES: u32 = 9;

	/// The number of blocks to wait for a threshold signature ceremony to complete.
	pub const THRESHOLD_SIGNATURE_CEREMONY_TIMEOUT_BLOCKS: u32 =
		((MAX_STAGE_DURATION_SECONDS * NUM_THRESHOLD_SIGNING_STAGES) + TIMEOUT_BUFFER_SECONDS) /
			SECONDS_PER_BLOCK as u32;

	/// The maximum number of blocks to wait for a keygen to complete.
	pub const KEYGEN_CEREMONY_TIMEOUT_BLOCKS: u32 =
		((MAX_STAGE_DURATION_SECONDS * NUM_KEYGEN_STAGES) + TIMEOUT_BUFFER_SECONDS) /
			SECONDS_PER_BLOCK as u32;

	/// Claims go live 48 hours after registration, so we need to allow enough time beyond that.
	pub const SECS_IN_AN_HOUR: u64 = 3600;
	// This should be the same as the `CLAIM_DELAY` in:
	// https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/contracts/StakeManager.sol
	pub const CLAIM_DELAY: u64 = 48 * SECS_IN_AN_HOUR;

	// NOTE: Currently it is not possible to change the slot duration after the chain has started.
	//       Attempting to do so will brick block production.
	pub const SLOT_DURATION: u64 = MILLISECONDS_PER_BLOCK;

	// Time is measured by number of blocks.
	pub const MINUTES: BlockNumber = 60_000 / (MILLISECONDS_PER_BLOCK as BlockNumber);
	pub const HOURS: BlockNumber = MINUTES * 60;
	pub const DAYS: BlockNumber = HOURS * 24;

	pub const EXPIRY_SPAN_IN_SECONDS: u64 = 80000;

	pub const CURRENT_AUTHORITY_EMISSION_INFLATION_BPS: u32 = 1000;
	pub const BACKUP_NODE_EMISSION_INFLATION_BPS: u32 = 100;

	/// The maximum number of broadcast attempts
	pub const MAXIMUM_BROADCAST_ATTEMPTS: AttemptCount = 100;

	/// The default minimum stake, 1_000 x 10^18
	pub const DEFAULT_MIN_STAKE: FlipBalance = 1_000 * 10u128.pow(18);

	/// Percent of the epoch we are allowed to claim
	pub const PERCENT_OF_EPOCH_PERIOD_CLAIMABLE: u8 = 50;

	/// The duration of the heartbeat interval in blocks. 150 blocks at a 6 second block time is
	/// equivalent to 15 minutes.
	pub const HEARTBEAT_BLOCK_INTERVAL: BlockNumber = 150;

	/// The mutliplier used to convert transaction weight into fees paid by the validators.
	///
	/// This can be used to estimate the value we put on our block execution times. We have 6
	/// seconds, and 1_000_000_000_000 weight units per block. We can extrapolate this to an epoch,
	/// and compare this to the rewards earned by validators over this period.
	///
	/// See https://github.com/chainflip-io/chainflip-backend/issues/1629
	pub const TX_FEE_MULTIPLIER: FlipBalance = 10_000;

	pub mod eth {
		use cf_chains::{Chain, Ethereum};

		/// Number of blocks to wait until we deem the block to be safe.
		pub const BLOCK_SAFETY_MARGIN: <Ethereum as Chain>::ChainBlockNumber = 4;
	}
}
