pub mod common {
	use cf_primitives::{AuthorityCount, BlockNumber, FlipBalance};

	pub const CHAINFLIP_SS58_PREFIX: u16 = 2112;

	const FLIP_DECIMALS: u32 = 18;
	pub const FLIPPERINOS_PER_FLIP: FlipBalance = 10u128.pow(FLIP_DECIMALS);

	pub const TOTAL_ISSUANCE: FlipBalance = {
		const TOTAL_ISSUANCE_IN_FLIP: FlipBalance = 90_000_000;
		TOTAL_ISSUANCE_IN_FLIP * FLIPPERINOS_PER_FLIP
	};

	pub const MAX_AUTHORITIES: AuthorityCount = 150;

	// Number of online credits required to get `ACCRUAL_REPUTATION_POINTS` of reputation
	const ACCRUAL_ONLINE_CREDITS: u32 = 2500;
	// Number of reputation points received for having `ACCRUAL_ONLINE_CREDITS`
	const ACCRUAL_REPUTATION_POINTS: i32 = 1;
	pub const ACCRUAL_RATIO: (i32, u32) = (ACCRUAL_REPUTATION_POINTS, ACCRUAL_ONLINE_CREDITS);

	/// This determines the average expected block time that we are targeting.
	/// Blocks will be produced at a minimum duration defined by `SLOT_DURATION`.
	/// `SLOT_DURATION` is picked up by `pallet_timestamp` which is in turn picked
	/// up by `pallet_aura` to implement `fn slot_duration()`.
	///
	/// Change this to adjust the block time.
	pub const MILLISECONDS_PER_BLOCK: u64 = 6000;

	pub const SECONDS_PER_BLOCK: u64 = MILLISECONDS_PER_BLOCK / 1000;

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

	pub const SECS_PER_MINUTE: u64 = 60;
	// This should be the same as the `REDEMPTION_DELAY` in:
	// https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/contracts/StateChainGateway.sol
	pub const REDEMPTION_DELAY_SECS: u64 = 5 * SECS_PER_MINUTE;

	// NOTE: Currently it is not possible to change the slot duration after the chain has started.
	//       Attempting to do so will brick block production.
	pub const SLOT_DURATION: u64 = MILLISECONDS_PER_BLOCK;

	// Time is measured by number of blocks.
	pub const MINUTES: BlockNumber = 60_000 / (MILLISECONDS_PER_BLOCK as BlockNumber);
	pub const HOURS: BlockNumber = MINUTES * 60;
	pub const DAYS: BlockNumber = HOURS * 24;

	pub const EXPIRY_SPAN_IN_SECONDS: u64 = 80000;

	/// Percent of the epoch we are allowed to redeem
	pub const PERCENT_OF_EPOCH_PERIOD_REDEEMABLE: u8 = 50;

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

	/// Default supply update interval is 24 hours.

	pub mod eth {
		use cf_chains::{eth::Ethereum, Chain};

		/// Number of blocks to wait until we deem the block to be safe.
		pub const BLOCK_SAFETY_MARGIN: <Ethereum as Chain>::ChainBlockNumber = 4;
	}

	pub mod btc {
		use cf_chains::{btc::Bitcoin, Chain};

		/// Number of blocks to wait for until we deem a BTC ingress to be safe.
		pub const INGRESS_BLOCK_SAFETY_MARGIN: <Bitcoin as Chain>::ChainBlockNumber = 3;
	}
}
