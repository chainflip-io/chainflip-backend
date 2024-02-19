pub mod common {
	use cf_primitives::{
		AuthorityCount, BlockNumber, FlipBalance, MILLISECONDS_PER_BLOCK, SECONDS_PER_BLOCK,
	};

	pub const CHAINFLIP_SS58_PREFIX: u16 = 2112;

	const FLIP_DECIMALS: u32 = 18;
	pub const FLIPPERINOS_PER_FLIP: FlipBalance = 10u128.pow(FLIP_DECIMALS);

	pub const TOTAL_ISSUANCE: FlipBalance = {
		const TOTAL_ISSUANCE_IN_FLIP: FlipBalance = 90_000_000;
		TOTAL_ISSUANCE_IN_FLIP * FLIPPERINOS_PER_FLIP
	};

	pub const MAX_AUTHORITIES: AuthorityCount = 150;

	// ======= Keygen and signing =======

	/// Maximum duration a ceremony stage can last
	pub const MAX_STAGE_DURATION_SECONDS: u32 = 30;

	const EXPECTED_FINALITY_DELAY_BLOCKS: u32 = 4;

	/// The transaction with the ceremony outcome needs some
	/// time to propagate to other nodes.
	const NETWORK_DELAY_SECONDS: u32 = 6;
	/// Buffer for final key computation.
	const KEY_DERIVATION_DELAY_SECONDS: u32 = 120;

	const TIMEOUT_BUFFER_SECONDS: u32 =
		EXPECTED_FINALITY_DELAY_BLOCKS * (SECONDS_PER_BLOCK as u32) + NETWORK_DELAY_SECONDS;

	const KEYGEN_TIMEOUT_BUFFER_SECONDS: u32 =
		TIMEOUT_BUFFER_SECONDS + KEY_DERIVATION_DELAY_SECONDS;

	const NUM_THRESHOLD_SIGNING_STAGES: u32 = 4;

	const NUM_KEYGEN_STAGES: u32 = 9;

	/// The number of blocks to wait for a threshold signature ceremony to complete.
	pub const THRESHOLD_SIGNATURE_CEREMONY_TIMEOUT_BLOCKS: u32 =
		((MAX_STAGE_DURATION_SECONDS * NUM_THRESHOLD_SIGNING_STAGES) + TIMEOUT_BUFFER_SECONDS) /
			SECONDS_PER_BLOCK as u32;

	/// The maximum number of blocks to wait for a keygen to complete.
	pub const KEYGEN_CEREMONY_TIMEOUT_BLOCKS: u32 = ((MAX_STAGE_DURATION_SECONDS *
		(NUM_KEYGEN_STAGES + NUM_THRESHOLD_SIGNING_STAGES)) +
		KEYGEN_TIMEOUT_BUFFER_SECONDS) /
		SECONDS_PER_BLOCK as u32;

	// NOTE: Currently it is not possible to change the slot duration after the chain has started.
	//       Attempting to do so will brick block production.
	pub const SLOT_DURATION: u64 = MILLISECONDS_PER_BLOCK;

	// Time is measured by number of blocks.
	pub const MINUTES: BlockNumber = 60_000 / (MILLISECONDS_PER_BLOCK as BlockNumber);
	pub const HOURS: BlockNumber = MINUTES * 60;
	pub const DAYS: BlockNumber = HOURS * 24;
	pub const YEAR: BlockNumber = DAYS * 365;

	/// Percent of the epoch we are allowed to redeem
	pub const REDEMPTION_PERIOD_AS_PERCENTAGE: u8 = 50;

	/// The duration of the heartbeat interval in blocks. 150 blocks at a 6 second block time is
	/// equivalent to 15 minutes.
	pub const HEARTBEAT_BLOCK_INTERVAL: BlockNumber = 150;

	/// The interval at which we update the per-block emission rate.
	///
	/// **Important**: If this constant is changed, we must also change
	/// [CURRENT_AUTHORITY_EMISSION_INFLATION_PERBILL] and [BACKUP_NODE_EMISSION_INFLATION_PERBILL].
	pub const COMPOUNDING_INTERVAL: u32 = HEARTBEAT_BLOCK_INTERVAL;

	/// The mutliplier used to convert transaction weight into fees paid by the validators.
	///
	/// This can be used to estimate the value we put on our block execution times. We have 6
	/// seconds, and 1_000_000_000_000 weight units per block. We can extrapolate this to an epoch,
	/// and compare this to the rewards earned by validators over this period.
	///
	/// See https://github.com/chainflip-io/chainflip-backend/issues/1629
	pub const TX_FEE_MULTIPLIER: FlipBalance = 10_000;

	/// The amount of time (in block number) allowed to pass from when a Witnessed call is
	/// dispatched to the witnessing deadline. After the deadline is passed, any authorities failed
	/// to witness the dispatched call are penalized.
	pub const LATE_WITNESS_GRACE_PERIOD: BlockNumber = 10u32;
}
