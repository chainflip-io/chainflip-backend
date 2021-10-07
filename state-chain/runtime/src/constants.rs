pub mod common {
	/// An index to a block.
	pub type BlockNumber = u32;
	pub type FlipBalance = u128;
	/// The type used as an epoch index.
	pub type EpochIndex = u32;
	pub type AuctionIndex = u64;

	pub const TOTAL_ISSUANCE: FlipBalance = {
		const TOKEN_ISSUANCE: FlipBalance = 90_000_000;
		const TOKEN_DECIMALS: u32 = 18;
		const TOKEN_FRACTIONS: FlipBalance = 10u128.pow(TOKEN_DECIMALS);
		TOKEN_ISSUANCE * TOKEN_FRACTIONS
	};

	pub const MAX_VALIDATORS: u32 = 150;

	pub const BLOCK_EMISSIONS: FlipBalance = {
		const ANNUAL_INFLATION_PERCENT: FlipBalance = 10;
		const ANNUAL_INFLATION: FlipBalance = TOTAL_ISSUANCE * ANNUAL_INFLATION_PERCENT / 100;
		// Note: DAYS is the number of blocks in a day.
		ANNUAL_INFLATION / 365 / DAYS as u128
	};

	// Number of blocks to be online to accrue a point
	pub const ACCRUAL_BLOCKS: u32 = 2500;
	// Number of accrual points
	pub const ACCRUAL_POINTS: i32 = 1;

	/// Claims go live 48 hours after registration, so we need to allow enough time beyond that.
	pub const SECS_IN_AN_HOUR: u64 = 3600;
	pub const REGISTRATION_DELAY: u64 = 48 * SECS_IN_AN_HOUR;
	/// This determines the average expected block time that we are targeting.
	/// Blocks will be produced at a minimum duration defined by `SLOT_DURATION`.
	/// `SLOT_DURATION` is picked up by `pallet_timestamp` which is in turn picked
	/// up by `pallet_aura` to implement `fn slot_duration()`.
	///
	/// Change this to adjust the block time.
	pub const MILLISECONDS_PER_BLOCK: u64 = 6000;

	pub const SLOT_DURATION: u64 = MILLISECONDS_PER_BLOCK;

	// Time is measured by number of blocks.
	pub const MINUTES: BlockNumber = 60_000 / (MILLISECONDS_PER_BLOCK as BlockNumber);
	pub const HOURS: BlockNumber = MINUTES * 60;
	pub const DAYS: BlockNumber = HOURS * 24;

	pub const EXPIRY_SPAN_IN_SECONDS: u64 = 80000;
}
