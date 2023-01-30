use super::*;

// UNISWAP TESTS => UniswapV3Pool.spec.ts

pub const TICKSPACING_UNISWAP_MEDIUM: Tick = 60;
pub const MIN_TICK_UNISWAP_MEDIUM: Tick = -887220;
pub const MAX_TICK_UNISWAP_MEDIUM: Tick = -MIN_TICK_UNISWAP_MEDIUM;

pub const INITIALIZE_LIQUIDITY_AMOUNT: u128 = 2000000000000000000u128;

// #Burn
fn pool_initialized_zerotick(
	mut pool: PoolState,
) -> (PoolState, PoolAssetMap<AmountU256>, AccountId) {
	let id: AccountId = AccountId::from([0xcf; 32]);
	let mut minted_capital = None;

	pool.mint(
		id.clone(),
		MIN_TICK_UNISWAP_MEDIUM,
		MAX_TICK_UNISWAP_MEDIUM,
		INITIALIZE_LIQUIDITY_AMOUNT,
		|minted| {
			minted_capital.replace(minted);
			Ok::<(), ()>(())
		},
	)
	.unwrap();
	let minted_capital = minted_capital.unwrap();

	(pool, minted_capital, id)
}

// Medium Fee, tickSpacing = 12, 1:1 price
fn mediumpool_initialized_zerotick() -> (PoolState, PoolAssetMap<AmountU256>, AccountId) {
	// fee_pips shall be one order of magnitude smaller than in the Uniswap pool (because
	// ONE_IN_HUNDREDTH_BIPS is /10)
	let pool = PoolState::new(3000, encodedprice1_1()).unwrap();
	pool_initialized_zerotick(pool)
}

fn checktickisclear(pool: &PoolState, tick: Tick) {
	match pool.liquidity_map.get(&tick) {
		None => {},
		_ => panic!("Expected NonExistent Key"),
	}
}

fn checkticknotclear(pool: &PoolState, tick: Tick) {
	if pool.liquidity_map.get(&tick).is_none() {
		panic!("Expected Key")
	}
}

fn mint_pool() -> (PoolState, PoolAssetMap<AmountU256>, AccountId) {
	let mut pool =
		PoolState::new(3000, U256::from_dec_str("25054144837504793118650146401").unwrap()).unwrap(); // encodeSqrtPrice (1,10)
	let id: AccountId = AccountId::from([0xcf; 32]);
	const MINTED_LIQUIDITY: u128 = 3_161;
	let mut minted_capital = None;

	let _ = pool.mint(
		id.clone(),
		MIN_TICK_UNISWAP_MEDIUM,
		MAX_TICK_UNISWAP_MEDIUM,
		MINTED_LIQUIDITY,
		|minted| {
			minted_capital.replace(minted);
			Ok::<(), ()>(())
		},
	);
	let minted_capital = minted_capital.unwrap();

	(pool, minted_capital, id)
}

#[test]
fn test_initialize_failure() {
	match PoolState::new(1000, U256::from(1)) {
		Err(CreatePoolError::InvalidInitialPrice) => {},
		_ => panic!("Fees accrued are not zero"),
	}
}
#[test]
fn test_initialize_success() {
	let _ = PoolState::new(1000, MIN_SQRT_PRICE);
	let _ = PoolState::new(1000, MAX_SQRT_PRICE - 1);

	let pool =
		PoolState::new(1000, U256::from_dec_str("56022770974786143748341366784").unwrap()).unwrap();

	assert_eq!(
		pool.current_sqrt_price,
		U256::from_dec_str("56022770974786143748341366784").unwrap()
	);
	assert_eq!(pool.current_tick, -6_932);
}
#[test]
fn test_initialize_too_low() {
	match PoolState::new(1000, MIN_SQRT_PRICE - 1) {
		Err(CreatePoolError::InvalidInitialPrice) => {},
		_ => panic!("Fees accrued are not zero"),
	}
}

#[test]
fn test_initialize_too_high() {
	match PoolState::new(1000, MAX_SQRT_PRICE) {
		Err(CreatePoolError::InvalidInitialPrice) => {},
		_ => panic!("Fees accrued are not zero"),
	}
}

#[test]
fn test_initialize_too_high_2() {
	match PoolState::new(
		1000,
		U256::from_dec_str(
			"57896044618658097711785492504343953926634992332820282019728792003956564819968", /* 2**160-1 */
		)
		.unwrap(),
	) {
		Err(CreatePoolError::InvalidInitialPrice) => {},
		_ => panic!("Fees accrued are not zero"),
	}
}

// Minting

#[test]
fn test_mint_err() {
	let (mut pool, _, id) = mint_pool();
	assert!(pool.mint(id.clone(), 1, 0, 1, |_| Ok::<(), ()>(())).is_err());
	assert!((pool.mint(id.clone(), -887273, 0, 1, |_| Ok::<(), ()>(()))).is_err());
	assert!((pool.mint(id.clone(), 0, 887273, 1, |_| Ok::<(), ()>(()))).is_err());

	assert!((pool.mint(
		id.clone(),
		MIN_TICK_UNISWAP_MEDIUM + 1,
		MAX_TICK_UNISWAP_MEDIUM - 1,
		MAX_TICK_GROSS_LIQUIDITY + 1,
		|_| Ok::<(), ()>(())
	))
	.is_err());

	assert!((pool.mint(
		id,
		MIN_TICK_UNISWAP_MEDIUM + 1,
		MAX_TICK_UNISWAP_MEDIUM - 1,
		MAX_TICK_GROSS_LIQUIDITY,
		|_| Ok::<(), ()>(())
	))
	.is_ok());
}

#[test]
fn test_mint_err_tickmax() {
	let (mut pool, _, id) = mint_pool();

	let (_, fees_owed) = pool
		.mint(id.clone(), MIN_TICK_UNISWAP_MEDIUM + 1, MAX_TICK_UNISWAP_MEDIUM - 1, 1000, |_| {
			Ok::<(), ()>(())
		})
		.unwrap();

	//assert_eq!(fees_owed.unwrap()[PoolSide::Asset0], 0);
	// assert_eq!(fees_owed.unwrap()[PoolSide::Asset1], 0);
	match (fees_owed[PoolSide::Asset0], fees_owed[PoolSide::Asset1]) {
		(0, 0) => {},
		_ => panic!("Fees accrued are not zero"),
	}

	assert!((pool.mint(
		id.clone(),
		MIN_TICK_UNISWAP_MEDIUM + 1,
		MAX_TICK_UNISWAP_MEDIUM - 1,
		MAX_TICK_GROSS_LIQUIDITY - 1000 + 1,
		|_| Ok::<(), ()>(())
	))
	.is_err());

	assert!((pool.mint(
		id.clone(),
		MIN_TICK_UNISWAP_MEDIUM + 2,
		MAX_TICK_UNISWAP_MEDIUM - 1,
		MAX_TICK_GROSS_LIQUIDITY - 1000 + 1,
		|_| Ok::<(), ()>(())
	))
	.is_err());

	assert!((pool.mint(
		id.clone(),
		MIN_TICK_UNISWAP_MEDIUM + 1,
		MAX_TICK_UNISWAP_MEDIUM - 2,
		MAX_TICK_GROSS_LIQUIDITY - 1000 + 1,
		|_| Ok::<(), ()>(())
	))
	.is_err());

	let (_, fees_owed) = pool
		.mint(
			id.clone(),
			MIN_TICK_UNISWAP_MEDIUM + 1,
			MAX_TICK_UNISWAP_MEDIUM - 1,
			MAX_TICK_GROSS_LIQUIDITY - 1000,
			|_| Ok::<(), ()>(()),
		)
		.unwrap();
	match (fees_owed[PoolSide::Asset0], fees_owed[PoolSide::Asset1]) {
		(0, 0) => {},
		_ => panic!("Fees accrued are not zero"),
	}

	// Different behaviour from Uniswap - does not revert when minting 0
	let (_, fees_owed) = pool
		.mint(id, MIN_TICK_UNISWAP_MEDIUM + 1, MAX_TICK_UNISWAP_MEDIUM - 1, 0, |_| Ok::<(), ()>(()))
		.unwrap();
	match (fees_owed[PoolSide::Asset0], fees_owed[PoolSide::Asset1]) {
		(0, 0) => {},
		_ => panic!("Fees accrued are not zero"),
	}
}

// Success cases

#[test]
fn test_balances() {
	let (_, minted_capital, _) = mint_pool();
	// Check "balances"
	const INPUT_TICKER: PoolSide = PoolSide::Asset0;
	assert_eq!(minted_capital[INPUT_TICKER], U256::from(9_996));
	assert_eq!(minted_capital[!INPUT_TICKER], U256::from(1_000));
}

#[test]
fn test_initial_tick() {
	let (pool, _, _) = mint_pool();
	// Check current tick
	assert_eq!(pool.current_tick, -23_028);
}

#[test]
fn above_current_price() {
	let (mut pool, mut minted_capital_accum, id) = mint_pool();

	const MINTED_LIQUIDITY: u128 = 10_000;
	const INPUT_TICKER: PoolSide = PoolSide::Asset0;

	let mut minted_capital = None;
	let (_, fees_owed) = pool
		.mint(id, -22980, 0, MINTED_LIQUIDITY, |minted| {
			minted_capital.replace(minted);
			Ok::<(), ()>(())
		})
		.unwrap();
	let minted_capital = minted_capital.unwrap();

	match (fees_owed[PoolSide::Asset0], fees_owed[PoolSide::Asset1]) {
		(0, 0) => {},
		_ => panic!("Fees accrued are not zero"),
	}

	assert_eq!(minted_capital[!INPUT_TICKER], U256::from(0));

	minted_capital_accum[INPUT_TICKER] += minted_capital[INPUT_TICKER];
	minted_capital_accum[!INPUT_TICKER] += minted_capital[!INPUT_TICKER];

	assert_eq!(minted_capital_accum[INPUT_TICKER], U256::from(9_996 + 21_549));
	assert_eq!(minted_capital_accum[!INPUT_TICKER], U256::from(1_000));
}

#[test]
fn test_maxtick_maxleverage() {
	let (mut pool, mut minted_capital_accum, id) = mint_pool();
	let mut minted_capital = None;
	let uniswap_max_tick = 887220;
	let uniswap_tickspacing = 60;
	pool.mint(
		id,
		uniswap_max_tick - uniswap_tickspacing, /* 60 == Uniswap's tickSpacing */
		uniswap_max_tick,
		5070602400912917605986812821504, /* 2**102 */
		|minted| {
			minted_capital.replace(minted);
			Ok::<(), ()>(())
		},
	)
	.unwrap();
	let minted_capital = minted_capital.unwrap();

	minted_capital_accum[PoolSide::Asset0] += minted_capital[PoolSide::Asset0];
	minted_capital_accum[!PoolSide::Asset0] += minted_capital[!PoolSide::Asset0];

	assert_eq!(minted_capital_accum[PoolSide::Asset0], U256::from(9_996 + 828_011_525));
	assert_eq!(minted_capital_accum[!PoolSide::Asset0], U256::from(1_000));
}

#[test]
fn test_maxtick() {
	let (mut pool, mut minted_capital_accum, id) = mint_pool();
	let mut minted_capital = None;
	pool.mint(id, -22980, 887220, 10000, |minted| {
		minted_capital.replace(minted);
		Ok::<(), ()>(())
	})
	.unwrap();
	let minted_capital = minted_capital.unwrap();

	minted_capital_accum[PoolSide::Asset0] += minted_capital[PoolSide::Asset0];
	minted_capital_accum[!PoolSide::Asset0] += minted_capital[!PoolSide::Asset0];

	assert_eq!(minted_capital_accum[PoolSide::Asset0], U256::from(9_996 + 31_549));
	assert_eq!(minted_capital_accum[!PoolSide::Asset0], U256::from(1_000));
}

#[test]
fn test_removing_works_0() {
	let (mut pool, _, id) = mint_pool();
	let mut minted_capital = None;
	pool.mint(id.clone(), -240, 0, 10000, |minted| {
		minted_capital.replace(minted);
		Ok::<(), ()>(())
	})
	.unwrap();

	let (returned_capital, fees_owed) = pool.burn(id, -240, 0, 10000).unwrap();

	assert_eq!(returned_capital[PoolSide::Asset0], U256::from(120));
	assert_eq!(returned_capital[!PoolSide::Asset0], U256::from(0));

	assert_eq!(fees_owed[PoolSide::Asset0], 0);
	assert_eq!(fees_owed[!PoolSide::Asset0], 0);
}

#[test]
fn test_removing_works_twosteps_0() {
	let (mut pool, _, id) = mint_pool();
	let mut minted_capital = None;
	pool.mint(id.clone(), -240, 0, 10000, |minted| {
		minted_capital.replace(minted);
		Ok::<(), ()>(())
	})
	.unwrap();

	let (returned_capital_0, fees_owed_0) = pool.burn(id.clone(), -240, 0, 10000 / 2).unwrap();
	let (returned_capital_1, fees_owed_1) = pool.burn(id, -240, 0, 10000 / 2).unwrap();

	assert_eq!(returned_capital_0[PoolSide::Asset0], U256::from(60));
	assert_eq!(returned_capital_0[!PoolSide::Asset0], U256::from(0));
	assert_eq!(returned_capital_1[PoolSide::Asset0], U256::from(60));
	assert_eq!(returned_capital_1[!PoolSide::Asset0], U256::from(0));

	assert_eq!(fees_owed_0[PoolSide::Asset0], 0);
	assert_eq!(fees_owed_0[!PoolSide::Asset0], 0);
	assert_eq!(fees_owed_1[PoolSide::Asset0], 0);
	assert_eq!(fees_owed_1[!PoolSide::Asset0], 0);
}

#[test]
fn test_addliquidityto_liquiditygross() {
	let (mut pool, _, id) = mint_pool();
	let (_, fees_owed) = pool.mint(id.clone(), -240, 0, 100, |_| Ok::<(), ()>(())).unwrap();

	match (fees_owed[PoolSide::Asset0], fees_owed[PoolSide::Asset1]) {
		(0, 0) => {},
		_ => panic!("Fees accrued are not zero"),
	}

	assert_eq!(pool.liquidity_map.get(&-240).unwrap().liquidity_gross, 100);
	assert_eq!(pool.liquidity_map.get(&0).unwrap().liquidity_gross, 100);
	assert!(!pool.liquidity_map.contains_key(&1));
	assert!(!pool.liquidity_map.contains_key(&2));

	let (_, fees_owed) = pool.mint(id.clone(), -240, 1, 150, |_| Ok::<(), ()>(())).unwrap();

	match (fees_owed[PoolSide::Asset0], fees_owed[PoolSide::Asset1]) {
		(0, 0) => {},
		_ => panic!("Fees accrued are not zero"),
	}
	assert_eq!(pool.liquidity_map.get(&-240).unwrap().liquidity_gross, 250);
	assert_eq!(pool.liquidity_map.get(&0).unwrap().liquidity_gross, 100);
	assert_eq!(pool.liquidity_map.get(&1).unwrap().liquidity_gross, 150);
	assert!(!pool.liquidity_map.contains_key(&2));

	let (_, fees_owed) = pool.mint(id, 0, 2, 60, |_| Ok::<(), ()>(())).unwrap();

	match (fees_owed[PoolSide::Asset0], fees_owed[PoolSide::Asset1]) {
		(0, 0) => {},
		_ => panic!("Fees accrued are not zero"),
	}
	assert_eq!(pool.liquidity_map.get(&-240).unwrap().liquidity_gross, 250);
	assert_eq!(pool.liquidity_map.get(&0).unwrap().liquidity_gross, 160);
	assert_eq!(pool.liquidity_map.get(&1).unwrap().liquidity_gross, 150);
	assert_eq!(pool.liquidity_map.get(&2).unwrap().liquidity_gross, 60);
}

#[test]
fn test_remove_liquidity_liquiditygross() {
	let (mut pool, _, id) = mint_pool();
	pool.mint(id.clone(), -240, 0, 100, |_| Ok::<(), ()>(())).unwrap();
	pool.mint(id.clone(), -240, 0, 40, |_| Ok::<(), ()>(())).unwrap();
	let (_, fees_owed) = pool.burn(id, -240, 0, 90).unwrap();
	match (fees_owed[PoolSide::Asset0], fees_owed[PoolSide::Asset1]) {
		(0, 0) => {},
		_ => panic!("Fees accrued are not zero"),
	}
	assert_eq!(pool.liquidity_map.get(&-240).unwrap().liquidity_gross, 50);
	assert_eq!(pool.liquidity_map.get(&0).unwrap().liquidity_gross, 50);
}

#[test]
fn test_clearsticklower_ifpositionremoved() {
	let (mut pool, _, id) = mint_pool();
	pool.mint(id.clone(), -240, 0, 100, |_| Ok::<(), ()>(())).unwrap();
	let (_, fees_owed) = pool.burn(id, -240, 0, 100).unwrap();
	match (fees_owed[PoolSide::Asset0], fees_owed[PoolSide::Asset1]) {
		(0, 0) => {},
		_ => panic!("Fees accrued are not zero"),
	}
	assert!(!pool.liquidity_map.contains_key(&-240));
}

#[test]
fn test_clearstickupper_ifpositionremoved() {
	let (mut pool, _, id) = mint_pool();
	pool.mint(id.clone(), -240, 0, 100, |_| Ok::<(), ()>(())).unwrap();
	pool.burn(id, -240, 0, 100).unwrap();
	assert!(!pool.liquidity_map.contains_key(&0));
}

#[test]
fn test_clears_onlyunused() {
	let (mut pool, _, id) = mint_pool();
	pool.mint(id.clone(), -240, 0, 100, |_| Ok::<(), ()>(())).unwrap();
	pool.mint(id.clone(), -60, 0, 250, |_| Ok::<(), ()>(())).unwrap();
	pool.burn(id, -240, 0, 100).unwrap();
	assert!(!pool.liquidity_map.contains_key(&-240));
	assert_eq!(pool.liquidity_map.get(&0).unwrap().liquidity_gross, 250);
	assert_eq!(
		pool.liquidity_map.get(&0).unwrap().fee_growth_outside[PoolSide::Asset0],
		U256::from(0)
	);
	assert_eq!(
		pool.liquidity_map.get(&0).unwrap().fee_growth_outside[!PoolSide::Asset0],
		U256::from(0)
	);
}

// Including current price

#[test]
fn test_price_within_range() {
	let (mut pool, minted_capital_accum, id) = mint_pool();
	let mut minted_capital = None;
	pool.mint(
		id,
		MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
		MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
		100,
		|minted| {
			minted_capital.replace(minted);
			Ok::<(), ()>(())
		},
	)
	.unwrap();
	let minted_capital = minted_capital.unwrap();

	assert_eq!(minted_capital[PoolSide::Asset0], U256::from(317));
	assert_eq!(minted_capital[!PoolSide::Asset0], U256::from(32));

	assert_eq!(
		minted_capital_accum[PoolSide::Asset0] + minted_capital[PoolSide::Asset0],
		U256::from(9_996 + 317)
	);
	assert_eq!(
		minted_capital_accum[!PoolSide::Asset0] + minted_capital[!PoolSide::Asset0],
		U256::from(1_000 + 32)
	);
}

#[test]
fn test_initializes_lowertick() {
	let (mut pool, _, id) = mint_pool();
	pool.mint(
		id,
		MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
		MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
		100,
		|_| Ok::<(), ()>(()),
	)
	.unwrap();
	assert_eq!(
		pool.liquidity_map
			.get(&(MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM))
			.unwrap()
			.liquidity_gross,
		100
	);
}

#[test]
fn test_initializes_uppertick() {
	let (mut pool, _, id) = mint_pool();
	pool.mint(
		id,
		MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
		MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
		100,
		|_| Ok::<(), ()>(()),
	)
	.unwrap();
	assert_eq!(
		pool.liquidity_map
			.get(&(MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM))
			.unwrap()
			.liquidity_gross,
		100
	);
}

#[test]
fn test_minmax_tick() {
	let (mut pool, minted_capital_accum, id) = mint_pool();
	let mut minted_capital = None;
	pool.mint(id, MIN_TICK_UNISWAP_MEDIUM, MAX_TICK_UNISWAP_MEDIUM, 10000, |minted| {
		minted_capital.replace(minted);
		Ok::<(), ()>(())
	})
	.unwrap();
	let minted_capital = minted_capital.unwrap();

	assert_eq!(minted_capital[PoolSide::Asset0], U256::from(31623));
	assert_eq!(minted_capital[!PoolSide::Asset0], U256::from(3163));

	assert_eq!(
		minted_capital_accum[PoolSide::Asset0] + minted_capital[PoolSide::Asset0],
		U256::from(9_996 + 31623)
	);
	assert_eq!(
		minted_capital_accum[!PoolSide::Asset0] + minted_capital[!PoolSide::Asset0],
		U256::from(1_000 + 3163)
	);
}

#[test]
fn test_removing() {
	let (mut pool, _, id) = mint_pool();
	pool.mint(
		id.clone(),
		MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
		MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
		100,
		|_| Ok::<(), ()>(()),
	)
	.unwrap();

	let (amounts_owed, _) = pool
		.burn(
			id.clone(),
			MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
			MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
			100,
		)
		.unwrap();

	assert_eq!(amounts_owed[PoolSide::Asset0], U256::from(316));
	assert_eq!(amounts_owed[!PoolSide::Asset0], U256::from(31));

	// DIFF: Burn will have burnt the entire position so it will be deleted.
	match pool.burn(
		id,
		MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
		MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
		1,
	) {
		Err(PositionError::NonExistent) => {},
		_ => panic!("Expected NonExistent"),
	}
}

// Below current price

#[test]
fn test_transfer_token1_only() {
	let (mut pool, minted_capital_accum, id) = mint_pool();
	let mut minted_capital = None;
	pool.mint(id, -46080, -23040, 10000, |minted| {
		minted_capital.replace(minted);
		Ok::<(), ()>(())
	})
	.unwrap();
	let minted_capital = minted_capital.unwrap();

	assert_eq!(minted_capital[PoolSide::Asset0], U256::from(0));
	assert_eq!(minted_capital[!PoolSide::Asset0], U256::from(2162));

	assert_eq!(
		minted_capital_accum[PoolSide::Asset0] + minted_capital[PoolSide::Asset0],
		U256::from(9_996)
	);
	assert_eq!(
		minted_capital_accum[!PoolSide::Asset0] + minted_capital[!PoolSide::Asset0],
		U256::from(1_000 + 2162)
	);
}

#[test]
fn test_mintick_maxleverage() {
	let (mut pool, minted_capital_accum, id) = mint_pool();
	let mut minted_capital = None;
	pool.mint(
		id,
		MIN_TICK_UNISWAP_MEDIUM,
		MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
		5070602400912917605986812821504, /* 2**102 */
		|minted| {
			minted_capital.replace(minted);
			Ok::<(), ()>(())
		},
	)
	.unwrap();
	let minted_capital = minted_capital.unwrap();

	assert_eq!(minted_capital[PoolSide::Asset0], U256::from(0));
	assert_eq!(minted_capital[!PoolSide::Asset0], U256::from(828011520));

	assert_eq!(
		minted_capital_accum[PoolSide::Asset0] + minted_capital[PoolSide::Asset0],
		U256::from(9_996)
	);
	assert_eq!(
		minted_capital_accum[!PoolSide::Asset0] + minted_capital[!PoolSide::Asset0],
		U256::from(1_000 + 828011520)
	);
}

#[test]
fn test_mintick() {
	let (mut pool, minted_capital_accum, id) = mint_pool();
	let mut minted_capital = None;
	pool.mint(id, MIN_TICK_UNISWAP_MEDIUM, -23040, 10000, |minted| {
		minted_capital.replace(minted);
		Ok::<(), ()>(())
	})
	.unwrap();
	let minted_capital = minted_capital.unwrap();

	assert_eq!(minted_capital[PoolSide::Asset0], U256::from(0));
	assert_eq!(minted_capital[!PoolSide::Asset0], U256::from(3161));

	assert_eq!(
		minted_capital_accum[PoolSide::Asset0] + minted_capital[PoolSide::Asset0],
		U256::from(9_996)
	);
	assert_eq!(
		minted_capital_accum[!PoolSide::Asset0] + minted_capital[!PoolSide::Asset0],
		U256::from(1_000 + 3161)
	);
}

#[test]
fn test_removing_works_1() {
	let (mut pool, _, id) = mint_pool();
	let mut minted_capital = None;
	pool.mint(id.clone(), -46080, -46020, 10000, |minted| {
		minted_capital.replace(minted);
		Ok::<(), ()>(())
	})
	.unwrap();

	let (returned_capital, fees_owed) = pool.burn(id.clone(), -46080, -46020, 10000).unwrap();

	// DIFF: Burn will have burnt the entire position so it will be deleted.
	assert_eq!(returned_capital[PoolSide::Asset0], U256::from(0));
	assert_eq!(returned_capital[!PoolSide::Asset0], U256::from(3));

	assert_eq!(fees_owed[PoolSide::Asset0], 0);
	assert_eq!(fees_owed[!PoolSide::Asset0], 0);

	match pool.burn(id, -46080, -46020, 1) {
		Err(PositionError::NonExistent) => {},
		_ => panic!("Expected NonExistent"),
	}
}

// NOTE: There is no implementation of protocol fees so we skip those tests

#[test]
fn test_poke_uninitialized_position() {
	let (mut pool, _, id) = mint_pool();
	pool.mint(
		AccountId::from([0xce; 32]),
		MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
		MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
		expandto18decimals(1).as_u128(),
		|_| Ok::<(), ()>(()),
	)
	.unwrap();

	let swap_input: u128 = expandto18decimals(1).as_u128();

	assert!(pool.swap::<Asset0ToAsset1>((swap_input / 10).into()).is_ok());
	assert!(pool.swap::<Asset1ToAsset0>((swap_input / 100).into()).is_ok());

	match pool.burn(
		id.clone(),
		MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
		MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
		0,
	) {
		Err(PositionError::NonExistent) => {},
		_ => panic!("Expected NonExistent"),
	}

	let (_, fees_owed) = pool
		.mint(
			id.clone(),
			MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
			MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
			1,
			|_| Ok::<(), ()>(()),
		)
		.unwrap();

	match (fees_owed[PoolSide::Asset0], fees_owed[PoolSide::Asset1]) {
		(0, 0) => {},
		_ => panic!("Fees accrued are not zero"),
	}

	let tick = pool
		.positions
		.get(&(
			id.clone(),
			MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
			MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
		))
		.unwrap();
	assert_eq!(tick.liquidity, 1);
	assert_eq!(
		tick.last_fee_growth_inside[PoolSide::Asset0],
		U256::from_dec_str("102084710076281216349243831104605583").unwrap()
	);
	assert_eq!(
		tick.last_fee_growth_inside[!PoolSide::Asset0],
		U256::from_dec_str("10208471007628121634924383110460558").unwrap()
	);
	// assert_eq!(tick.fees_owed[PoolSide::Asset0], 0);
	// assert_eq!(tick.fees_owed[!PoolSide::Asset0], 0);

	let (returned_capital, fees_owed) = pool
		.burn(
			id.clone(),
			MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
			MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
			1,
		)
		.unwrap();

	// DIFF: Burn will have burnt the entire position so it will be deleted.
	assert_eq!(fees_owed[PoolSide::Asset0], 0);
	assert_eq!(fees_owed[!PoolSide::Asset0], 0);

	// This could be missing + fees_owed[PoolSide::Asset0]
	assert_eq!(returned_capital[PoolSide::Asset0], U256::from(3));
	assert_eq!(returned_capital[!PoolSide::Asset0], U256::from(0));

	match pool.positions.get(&(
		id,
		MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
		MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
	)) {
		None => {},
		_ => panic!("Expected NonExistent Key"),
	}
}

// Own test
#[test]
fn test_multiple_burns() {
	let (mut pool, _, _id) = mediumpool_initialized_zerotick();
	// some activity that would make the ticks non-zero
	pool.mint(
		AccountId::from([0xce; 32]),
		MIN_TICK_UNISWAP_MEDIUM,
		MAX_TICK_UNISWAP_MEDIUM,
		expandto18decimals(1).as_u128(),
		|_| Ok::<(), ()>(()),
	)
	.unwrap();

	assert!(pool.swap::<Asset0ToAsset1>(expandto18decimals(1)).is_ok());
	assert!(pool.swap::<Asset1ToAsset0>(expandto18decimals(1)).is_ok());

	// Should be able to do only 1 burn (1000000000000000000 / 987654321000000000)

	pool.burn(
		AccountId::from([0xce; 32]),
		MIN_TICK_UNISWAP_MEDIUM,
		MAX_TICK_UNISWAP_MEDIUM,
		987654321000000000,
	)
	.unwrap();

	match pool.burn(
		AccountId::from([0xce; 32]),
		MIN_TICK_UNISWAP_MEDIUM,
		MAX_TICK_UNISWAP_MEDIUM,
		987654321000000000,
	) {
		Err(PositionError::PositionLacksLiquidity) => {},
		_ => panic!("Expected InsufficientLiquidity"),
	}
}

#[test]
fn test_notclearposition_ifnomoreliquidity() {
	let (mut pool, _, _id) = mediumpool_initialized_zerotick();
	// some activity that would make the ticks non-zero
	pool.mint(
		AccountId::from([0xce; 32]),
		MIN_TICK_UNISWAP_MEDIUM,
		MAX_TICK_UNISWAP_MEDIUM,
		expandto18decimals(1).as_u128(),
		|_| Ok::<(), ()>(()),
	)
	.unwrap();

	assert!(pool.swap::<Asset0ToAsset1>(expandto18decimals(1)).is_ok());
	assert!(pool.swap::<Asset1ToAsset0>(expandto18decimals(1)).is_ok());

	// Add a poke to update the fee growth and check it's value
	let (returned_capital, fees_owed) = pool
		.burn(AccountId::from([0xce; 32]), MIN_TICK_UNISWAP_MEDIUM, MAX_TICK_UNISWAP_MEDIUM, 0)
		.unwrap();

	assert_ne!(fees_owed[PoolSide::Asset0], 0);
	assert_ne!(fees_owed[!PoolSide::Asset0], 0);
	assert_eq!(returned_capital[PoolSide::Asset0], U256::from(0));
	assert_eq!(returned_capital[!PoolSide::Asset0], U256::from(0));

	let pos = pool
		.positions
		.get(&(AccountId::from([0xce; 32]), MIN_TICK_UNISWAP_MEDIUM, MAX_TICK_UNISWAP_MEDIUM))
		.unwrap();
	assert_eq!(
		pos.last_fee_growth_inside[PoolSide::Asset0],
		U256::from_dec_str("340282366920938463463374607431768211").unwrap()
	);
	assert_eq!(
		pos.last_fee_growth_inside[!PoolSide::Asset0],
		U256::from_dec_str("340282366920938463463374607431768211").unwrap()
	);

	let (returned_capital, fees_owed) = pool
		.burn(
			AccountId::from([0xce; 32]),
			MIN_TICK_UNISWAP_MEDIUM,
			MAX_TICK_UNISWAP_MEDIUM,
			expandto18decimals(1).as_u128(),
		)
		.unwrap();

	// DIFF: Burn will have burnt the entire position so it will be deleted.
	// Also, fees will already have been collected in the first burn.
	assert_eq!(fees_owed[PoolSide::Asset0], 0);
	assert_eq!(fees_owed[!PoolSide::Asset0], 0);

	// This could be missing + fees_owed[PoolSide::Asset0]
	assert_ne!(returned_capital[PoolSide::Asset0], U256::from(0));
	assert_ne!(returned_capital[!PoolSide::Asset0], U256::from(0));

	match pool.positions.get(&(
		AccountId::from([0xce; 32]),
		MIN_TICK_UNISWAP_MEDIUM,
		MAX_TICK_UNISWAP_MEDIUM,
	)) {
		None => {},
		_ => panic!("Expected NonExistent Key"),
	}
}

#[test]
fn test_clearstick_iflastposition() {
	let (mut pool, _, id) = mediumpool_initialized_zerotick();
	// some activity that would make the ticks non-zero
	pool.mint(
		id.clone(),
		MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
		MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
		1,
		|_| Ok::<(), ()>(()),
	)
	.unwrap();

	assert!(pool.swap::<Asset0ToAsset1>(expandto18decimals(1)).is_ok());

	pool.burn(
		id,
		MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
		MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
		1,
	)
	.unwrap();

	checktickisclear(&pool, MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM);
	checktickisclear(&pool, MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM);
}

#[test]
fn test_clearlower_ifupperused() {
	let (mut pool, _, id) = mediumpool_initialized_zerotick();
	// some activity that would make the ticks non-zero
	pool.mint(
		id.clone(),
		MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
		MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
		1,
		|_| Ok::<(), ()>(()),
	)
	.unwrap();
	pool.mint(
		id.clone(),
		MIN_TICK_UNISWAP_MEDIUM + 2 * TICKSPACING_UNISWAP_MEDIUM,
		MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
		1,
		|_| Ok::<(), ()>(()),
	)
	.unwrap();
	assert!(pool.swap::<Asset0ToAsset1>(expandto18decimals(1)).is_ok());

	pool.burn(
		id,
		MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
		MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
		1,
	)
	.unwrap();

	checktickisclear(&pool, MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM);
	checkticknotclear(&pool, MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM);
}

#[test]
fn test_clearupper_iflowerused() {
	let (mut pool, _, id) = mediumpool_initialized_zerotick();
	// some activity that would make the ticks non-zero
	pool.mint(
		id.clone(),
		MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
		MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
		1,
		|_| Ok::<(), ()>(()),
	)
	.unwrap();
	pool.mint(
		id.clone(),
		MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
		MAX_TICK_UNISWAP_MEDIUM - 2 * TICKSPACING_UNISWAP_MEDIUM,
		1,
		|_| Ok::<(), ()>(()),
	)
	.unwrap();
	assert!(pool.swap::<Asset0ToAsset1>(expandto18decimals(1)).is_ok());

	pool.burn(
		id,
		MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
		MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
		1,
	)
	.unwrap();

	checkticknotclear(&pool, MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM);
	checktickisclear(&pool, MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM);
}

pub const TICKSPACING_UNISWAP_LOW: Tick = 10;
pub const MIN_TICK_UNISWAP_LOW: Tick = -887220;
pub const MAX_TICK_UNISWAP_LOW: Tick = -MIN_TICK_UNISWAP_LOW;

// Low Fee, tickSpacing = 10, 1:1 price
fn lowpool_initialized_zerotick() -> (PoolState, PoolAssetMap<AmountU256>, AccountId) {
	// Tickspacing
	let pool = PoolState::new(500, encodedprice1_1()).unwrap(); //	encodeSqrtPrice (1,1)
	pool_initialized_zerotick(pool)
}

#[test]
fn test_mint_rightofcurrentprice() {
	let (mut pool, _, id) = lowpool_initialized_zerotick();

	let liquiditybefore = pool.current_liquidity;

	let mut minted_capital = None;
	pool.mint(id, TICKSPACING_UNISWAP_LOW, 2 * TICKSPACING_UNISWAP_LOW, 1000, |minted| {
		minted_capital.replace(minted);
		Ok::<(), ()>(())
	})
	.unwrap();
	let minted_capital = minted_capital.unwrap();

	assert!(pool.current_liquidity >= liquiditybefore);

	assert_eq!(minted_capital[PoolSide::Asset0], U256::from(1));
	assert_eq!(minted_capital[!PoolSide::Asset0], U256::from(0));
}

#[test]
fn test_mint_leftofcurrentprice() {
	let (mut pool, _, id) = lowpool_initialized_zerotick();

	let liquiditybefore = pool.current_liquidity;

	let mut minted_capital = None;
	pool.mint(id, -2 * TICKSPACING_UNISWAP_LOW, -TICKSPACING_UNISWAP_LOW, 1000, |minted| {
		minted_capital.replace(minted);
		Ok::<(), ()>(())
	})
	.unwrap();
	let minted_capital = minted_capital.unwrap();

	assert!(pool.current_liquidity >= liquiditybefore);

	assert_eq!(minted_capital[PoolSide::Asset0], U256::from(0));
	assert_eq!(minted_capital[!PoolSide::Asset0], U256::from(1));
}

#[test]
fn test_mint_withincurrentprice() {
	let (mut pool, _, id) = lowpool_initialized_zerotick();

	let liquiditybefore = pool.current_liquidity;

	let mut minted_capital = None;
	pool.mint(id, -TICKSPACING_UNISWAP_LOW, TICKSPACING_UNISWAP_LOW, 1000, |minted| {
		minted_capital.replace(minted);
		Ok::<(), ()>(())
	})
	.unwrap();
	let minted_capital = minted_capital.unwrap();

	assert!(pool.current_liquidity >= liquiditybefore);

	assert_eq!(minted_capital[PoolSide::Asset0], U256::from(1));
	assert_eq!(minted_capital[!PoolSide::Asset0], U256::from(1));
}

#[test]
fn test_cannotremove_morethanposition() {
	let (mut pool, _, id) = lowpool_initialized_zerotick();

	pool.mint(
		id.clone(),
		-TICKSPACING_UNISWAP_LOW,
		TICKSPACING_UNISWAP_LOW,
		expandto18decimals(1).as_u128(),
		|_| Ok::<(), ()>(()),
	)
	.unwrap();

	match pool.burn(
		id,
		-TICKSPACING_UNISWAP_LOW,
		TICKSPACING_UNISWAP_LOW,
		expandto18decimals(1).as_u128() + 1,
	) {
		Err(PositionError::PositionLacksLiquidity) => {},
		_ => panic!("Should not be able to remove more than position"),
	}
}

#[test]
fn test_collectfees_withincurrentprice() {
	let (mut pool, _, id) = lowpool_initialized_zerotick();

	pool.mint(
		id.clone(),
		-TICKSPACING_UNISWAP_LOW * 100,
		TICKSPACING_UNISWAP_LOW * 100,
		expandto18decimals(100).as_u128(),
		|_| Ok::<(), ()>(()),
	)
	.unwrap();

	let liquiditybefore = pool.current_liquidity;
	assert!(pool.swap::<Asset0ToAsset1>(expandto18decimals(1)).is_ok());

	assert!(pool.current_liquidity >= liquiditybefore);

	// Poke
	let (returned_capital, fees_owed) = pool
		.burn(id, -TICKSPACING_UNISWAP_LOW * 100, TICKSPACING_UNISWAP_LOW * 100, 0)
		.unwrap();

	assert_eq!(returned_capital[PoolSide::Asset0], U256::from(0));
	assert_eq!(returned_capital[!PoolSide::Asset0], U256::from(0));

	assert!(fees_owed[PoolSide::Asset0] > 0);
	assert_eq!(fees_owed[!PoolSide::Asset0], 0);
}

// Post initialize at medium fee

#[test]
fn test_initial_liquidity() {
	let (pool, _, _) = mediumpool_initialized_zerotick();
	assert_eq!(pool.current_liquidity, expandto18decimals(2).as_u128());
}

#[test]
fn test_returns_insupply_inrange() {
	let (mut pool, _, id) = mediumpool_initialized_zerotick();
	pool.mint(
		id,
		-TICKSPACING_UNISWAP_MEDIUM,
		TICKSPACING_UNISWAP_MEDIUM,
		expandto18decimals(3).as_u128(),
		|_| Ok::<(), ()>(()),
	)
	.unwrap();
	assert_eq!(pool.current_liquidity, expandto18decimals(5).as_u128());
}

#[test]
fn test_excludes_supply_abovetick() {
	let (mut pool, _, id) = mediumpool_initialized_zerotick();
	pool.mint(
		id,
		TICKSPACING_UNISWAP_MEDIUM,
		2 * TICKSPACING_UNISWAP_MEDIUM,
		expandto18decimals(3).as_u128(),
		|_| Ok::<(), ()>(()),
	)
	.unwrap();
	assert_eq!(pool.current_liquidity, expandto18decimals(2).as_u128());
}

#[test]
fn test_excludes_supply_belowtick() {
	let (mut pool, _, id) = mediumpool_initialized_zerotick();
	pool.mint(
		id,
		-2 * TICKSPACING_UNISWAP_MEDIUM,
		-TICKSPACING_UNISWAP_MEDIUM,
		expandto18decimals(3).as_u128(),
		|_| Ok::<(), ()>(()),
	)
	.unwrap();
	assert_eq!(pool.current_liquidity, expandto18decimals(2).as_u128());
}

#[test]
fn test_updates_exiting() {
	let (mut pool, _, id) = mediumpool_initialized_zerotick();
	assert_eq!(pool.current_liquidity, expandto18decimals(2).as_u128());

	pool.mint(id, 0, TICKSPACING_UNISWAP_MEDIUM, expandto18decimals(1).as_u128(), |_| {
		Ok::<(), ()>(())
	})
	.unwrap();
	assert_eq!(pool.current_liquidity, expandto18decimals(3).as_u128());

	// swap toward the left (just enough for the tick transition function to trigger)
	assert!(pool.swap::<Asset0ToAsset1>((1).into()).is_ok());

	assert_eq!(pool.current_tick, -1);
	assert_eq!(pool.current_liquidity, expandto18decimals(2).as_u128());
}

#[test]
fn test_updates_entering() {
	let (mut pool, _, id) = mediumpool_initialized_zerotick();
	assert_eq!(pool.current_liquidity, expandto18decimals(2).as_u128());

	pool.mint(id, -TICKSPACING_UNISWAP_MEDIUM, 0, expandto18decimals(1).as_u128(), |_| {
		Ok::<(), ()>(())
	})
	.unwrap();
	assert_eq!(pool.current_liquidity, expandto18decimals(2).as_u128());

	// swap toward the left (just enough for the tick transition function to trigger)
	assert!(pool.swap::<Asset0ToAsset1>((1).into()).is_ok());

	assert_eq!(pool.current_tick, -1);
	assert_eq!(pool.current_liquidity, expandto18decimals(3).as_u128());
}

// Uniswap "limit orders"

#[test]
fn test_limitselling_asset_0_to_asset1_tick0thru1() {
	let (mut pool, _, id) = mediumpool_initialized_zerotick();
	let mut minted_capital = None;
	pool.mint(id.clone(), 0, 120, expandto18decimals(1).as_u128(), |minted| {
		minted_capital.replace(minted);
		Ok::<(), ()>(())
	})
	.unwrap();
	let minted_capital = minted_capital.unwrap();

	assert_eq!(minted_capital[PoolSide::Asset0], U256::from_dec_str("5981737760509663").unwrap());
	assert_eq!(minted_capital[!PoolSide::Asset0], U256::from_dec_str("0").unwrap());

	// somebody takes the limit order
	assert!(pool
		.swap::<Asset1ToAsset0>(U256::from_dec_str("2000000000000000000").unwrap())
		.is_ok());

	let (burned, fees_owed) =
		pool.burn(id.clone(), 0, 120, expandto18decimals(1).as_u128()).unwrap();
	assert_eq!(burned[PoolSide::Asset0], U256::from_dec_str("0").unwrap());
	assert_eq!(burned[!PoolSide::Asset0], U256::from_dec_str("6017734268818165").unwrap());

	// DIFF: position fully burnt
	assert_eq!(fees_owed[PoolSide::Asset0], 0);
	assert_eq!(fees_owed[!PoolSide::Asset0], 18107525382602);

	match pool.burn(id, 0, 120, 1) {
		Err(PositionError::NonExistent) => {},
		_ => panic!("Expected NonExistent"),
	}

	assert!(pool.current_tick > 120)
}

#[test]
fn test_limitselling_asset_0_to_asset_1_tick0thru1_poke() {
	let (mut pool, _, id) = mediumpool_initialized_zerotick();
	let mut minted_capital = None;
	pool.mint(id.clone(), 0, 120, expandto18decimals(1).as_u128(), |minted| {
		minted_capital.replace(minted);
		Ok::<(), ()>(())
	})
	.unwrap();
	let minted_capital = minted_capital.unwrap();

	assert_eq!(minted_capital[PoolSide::Asset0], U256::from_dec_str("5981737760509663").unwrap());
	assert_eq!(minted_capital[!PoolSide::Asset0], U256::from_dec_str("0").unwrap());

	// somebody takes the limit order
	assert!(pool
		.swap::<Asset1ToAsset0>(U256::from_dec_str("2000000000000000000").unwrap())
		.is_ok());

	let (burned, fees_owed) = pool.burn(id.clone(), 0, 120, 0).unwrap();
	assert_eq!(burned[PoolSide::Asset0], U256::from_dec_str("0").unwrap());
	assert_eq!(burned[!PoolSide::Asset0], U256::from_dec_str("0").unwrap());

	// DIFF: position fully burnt
	assert_eq!(fees_owed[PoolSide::Asset0], 0);
	assert_eq!(fees_owed[!PoolSide::Asset0], 18107525382602);

	let (burned, fees_owed) = pool.burn(id, 0, 120, expandto18decimals(1).as_u128()).unwrap();
	assert_eq!(burned[PoolSide::Asset0], U256::from_dec_str("0").unwrap());
	assert_eq!(burned[!PoolSide::Asset0], U256::from_dec_str("6017734268818165").unwrap());

	// DIFF: position fully burnt
	assert_eq!(fees_owed[PoolSide::Asset0], 0);
	assert_eq!(fees_owed[!PoolSide::Asset0], 0);

	assert!(pool.current_tick > 120)
}

#[test]
fn test_limitselling_asset_1_to_asset_0_tick1thru0() {
	let (mut pool, _, id) = mediumpool_initialized_zerotick();
	let mut minted_capital = None;
	pool.mint(id.clone(), -120, 0, expandto18decimals(1).as_u128(), |minted| {
		minted_capital.replace(minted);
		Ok::<(), ()>(())
	})
	.unwrap();
	let minted_capital = minted_capital.unwrap();

	assert_eq!(minted_capital[!PoolSide::Asset0], U256::from_dec_str("5981737760509663").unwrap());
	assert_eq!(minted_capital[PoolSide::Asset0], U256::from_dec_str("0").unwrap());

	// somebody takes the limit order
	assert!(pool
		.swap::<Asset0ToAsset1>(U256::from_dec_str("2000000000000000000").unwrap())
		.is_ok());

	let (burned, fees_owed) =
		pool.burn(id.clone(), -120, 0, expandto18decimals(1).as_u128()).unwrap();
	assert_eq!(burned[!PoolSide::Asset0], U256::from_dec_str("0").unwrap());
	assert_eq!(burned[PoolSide::Asset0], U256::from_dec_str("6017734268818165").unwrap());

	// DIFF: position fully burnt
	assert_eq!(fees_owed[!PoolSide::Asset0], 0);
	assert_eq!(fees_owed[PoolSide::Asset0], 18107525382602);

	match pool.burn(id, -120, 0, 1) {
		Err(PositionError::NonExistent) => {},
		_ => panic!("Expected NonExistent"),
	}

	assert!(pool.current_tick < -120)
}

#[test]
fn test_limitselling_asset_1_to_asset_0_tick1thru0_poke() {
	let (mut pool, _, id) = mediumpool_initialized_zerotick();
	let mut minted_capital = None;
	pool.mint(id.clone(), -120, 0, expandto18decimals(1).as_u128(), |minted| {
		minted_capital.replace(minted);
		Ok::<(), ()>(())
	})
	.unwrap();
	let minted_capital = minted_capital.unwrap();

	assert_eq!(minted_capital[!PoolSide::Asset0], U256::from_dec_str("5981737760509663").unwrap());
	assert_eq!(minted_capital[PoolSide::Asset0], U256::from_dec_str("0").unwrap());

	// somebody takes the limit order
	assert!(pool
		.swap::<Asset0ToAsset1>(U256::from_dec_str("2000000000000000000").unwrap())
		.is_ok());

	let (burned, fees_owed) = pool.burn(id.clone(), -120, 0, 0).unwrap();
	assert_eq!(burned[!PoolSide::Asset0], U256::from_dec_str("0").unwrap());
	assert_eq!(burned[PoolSide::Asset0], U256::from_dec_str("0").unwrap());

	assert_eq!(fees_owed[!PoolSide::Asset0], 0);
	assert_eq!(fees_owed[PoolSide::Asset0], 18107525382602);

	let (burned, fees_owed) =
		pool.burn(id.clone(), -120, 0, expandto18decimals(1).as_u128()).unwrap();
	assert_eq!(burned[!PoolSide::Asset0], U256::from_dec_str("0").unwrap());
	assert_eq!(burned[PoolSide::Asset0], U256::from_dec_str("6017734268818165").unwrap());

	// DIFF: position fully burnt
	assert_eq!(fees_owed[!PoolSide::Asset0], 0);
	assert_eq!(fees_owed[PoolSide::Asset0], 0);

	match pool.burn(id, -120, 0, 1) {
		Err(PositionError::NonExistent) => {},
		_ => panic!("Expected NonExistent"),
	}

	assert!(pool.current_tick < -120)
}

// #Collect

// Low Fee, tickSpacing = 10, 1:1 price
fn lowpool_initialized_one() -> (PoolState, PoolAssetMap<AmountU256>, AccountId) {
	let pool = PoolState::new(500, encodedprice1_1()).unwrap();
	let id: AccountId = AccountId::from([0xcf; 32]);
	let minted_amounts: PoolAssetMap<AmountU256> = Default::default();
	(pool, minted_amounts, id)
}

#[test]
fn test_multiplelps() {
	let (mut pool, _, id) = lowpool_initialized_one();

	pool.mint(
		id.clone(),
		MIN_TICK_UNISWAP_LOW,
		MAX_TICK_UNISWAP_LOW,
		expandto18decimals(1).as_u128(),
		|_| Ok::<(), ()>(()),
	)
	.unwrap();
	pool.mint(
		id.clone(),
		MIN_TICK_UNISWAP_LOW + TICKSPACING_UNISWAP_LOW,
		MAX_TICK_UNISWAP_LOW - TICKSPACING_UNISWAP_LOW,
		2000000000000000000,
		|_| Ok::<(), ()>(()),
	)
	.unwrap();

	assert!(pool.swap::<Asset0ToAsset1>(expandto18decimals(1)).is_ok());

	// poke positions
	let (burned, fees_owed) =
		pool.burn(id.clone(), MIN_TICK_UNISWAP_LOW, MAX_TICK_UNISWAP_LOW, 0).unwrap();

	assert_eq!(burned[PoolSide::Asset0], U256::from_dec_str("0").unwrap());
	assert_eq!(burned[!PoolSide::Asset0], U256::from_dec_str("0").unwrap());
	// NOTE: Fee_owed value 1 unit different than Uniswap because uniswap requires 4 loops to do
	// the swap instead of 1 causing the rounding to be different
	assert_eq!(fees_owed[PoolSide::Asset0], 166666666666666u128);
	assert_eq!(fees_owed[!PoolSide::Asset0], 0);

	let (_, fees_owed) = pool
		.burn(
			id,
			MIN_TICK_UNISWAP_LOW + TICKSPACING_UNISWAP_LOW,
			MAX_TICK_UNISWAP_LOW - TICKSPACING_UNISWAP_LOW,
			0,
		)
		.unwrap();
	// NOTE: Fee_owed value 1 unit different than Uniswap because uniswap requires 4 loops to do
	// the swap instead of 1 causing the rounding to be different
	assert_eq!(fees_owed[PoolSide::Asset0], 333333333333333);
	assert_eq!(fees_owed[!PoolSide::Asset0], 0);
}

// Works across large increases
#[test]
fn test_before_capbidn() {
	let (mut pool, _, id) = lowpool_initialized_one();
	pool.mint(
		id.clone(),
		MIN_TICK_UNISWAP_LOW,
		MAX_TICK_UNISWAP_LOW,
		expandto18decimals(1).as_u128(),
		|_| Ok::<(), ()>(()),
	)
	.unwrap();

	pool.global_fee_growth[PoolSide::Asset0] =
		U256::from_dec_str("115792089237316195423570985008687907852929702298719625575994").unwrap();

	let (burned, fees_owed) = pool.burn(id, MIN_TICK_UNISWAP_LOW, MAX_TICK_UNISWAP_LOW, 0).unwrap();

	assert_eq!(burned[PoolSide::Asset0], U256::from_dec_str("0").unwrap());
	assert_eq!(burned[!PoolSide::Asset0], U256::from_dec_str("0").unwrap());

	assert_eq!(fees_owed[PoolSide::Asset0], u128::MAX - 1);
	assert_eq!(fees_owed[!PoolSide::Asset0], 0);
}

#[test]
fn test_after_capbidn() {
	let (mut pool, _, id) = lowpool_initialized_one();
	pool.mint(
		id.clone(),
		MIN_TICK_UNISWAP_LOW,
		MAX_TICK_UNISWAP_LOW,
		expandto18decimals(1).as_u128(),
		|_| Ok::<(), ()>(()),
	)
	.unwrap();

	pool.global_fee_growth[PoolSide::Asset0] =
		U256::from_dec_str("115792089237316195423570985008687907852929702298719625575995").unwrap();

	let (burned, fees_owed) = pool.burn(id, MIN_TICK_UNISWAP_LOW, MAX_TICK_UNISWAP_LOW, 0).unwrap();

	assert_eq!(burned[PoolSide::Asset0], U256::from_dec_str("0").unwrap());
	assert_eq!(burned[!PoolSide::Asset0], U256::from_dec_str("0").unwrap());

	assert_eq!(fees_owed[PoolSide::Asset0], u128::MAX);
	assert_eq!(fees_owed[!PoolSide::Asset0], 0);
}

#[test]
fn test_wellafter_capbidn() {
	let (mut pool, _, id) = lowpool_initialized_one();
	pool.mint(
		id.clone(),
		MIN_TICK_UNISWAP_LOW,
		MAX_TICK_UNISWAP_LOW,
		expandto18decimals(1).as_u128(),
		|_| Ok::<(), ()>(()),
	)
	.unwrap();

	pool.global_fee_growth[PoolSide::Asset0] = U256::MAX;

	let (burned, fees_owed) = pool.burn(id, MIN_TICK_UNISWAP_LOW, MAX_TICK_UNISWAP_LOW, 0).unwrap();

	assert_eq!(burned[PoolSide::Asset0], U256::from_dec_str("0").unwrap());
	assert_eq!(burned[!PoolSide::Asset0], U256::from_dec_str("0").unwrap());

	assert_eq!(fees_owed[PoolSide::Asset0], u128::MAX);
	assert_eq!(fees_owed[!PoolSide::Asset0], 0);
}

// DIFF: pool.global_fee_growth won't overflow. We make it saturate.

fn lowpool_initialized_setfees() -> (PoolState, PoolAssetMap<AmountU256>, AccountId) {
	let (mut pool, mut minted_amounts_accum, id) = lowpool_initialized_one();
	pool.global_fee_growth[PoolSide::Asset0] = U256::MAX;
	pool.global_fee_growth[!PoolSide::Asset0] = U256::MAX;

	let mut minted_capital = None;
	pool.mint(
		id.clone(),
		MIN_TICK_UNISWAP_LOW,
		MAX_TICK_UNISWAP_LOW,
		expandto18decimals(10).as_u128(),
		|minted| {
			minted_capital.replace(minted);
			Ok::<(), ()>(())
		},
	)
	.unwrap();
	let minted_capital = minted_capital.unwrap();

	minted_amounts_accum[PoolSide::Asset0] += minted_capital[PoolSide::Asset0];
	minted_amounts_accum[!PoolSide::Asset0] += minted_capital[!PoolSide::Asset0];

	(pool, minted_amounts_accum, id)
}

#[test]
fn test_base() {
	let (mut pool, _, id) = lowpool_initialized_setfees();

	assert!(pool.swap::<Asset0ToAsset1>(expandto18decimals(1)).is_ok());

	assert_eq!(pool.global_fee_growth[PoolSide::Asset0], U256::MAX);
	assert_eq!(pool.global_fee_growth[!PoolSide::Asset0], U256::MAX);

	let (_, fees_owed) = pool.burn(id, MIN_TICK_UNISWAP_LOW, MAX_TICK_UNISWAP_LOW, 0).unwrap();

	// DIFF: no fees accrued
	assert_eq!(fees_owed[PoolSide::Asset0], 0);
	assert_eq!(fees_owed[!PoolSide::Asset0], 0);
}

#[test]
fn test_pair() {
	let (mut pool, _, id) = lowpool_initialized_setfees();

	assert!(pool.swap::<Asset1ToAsset0>(expandto18decimals(1)).is_ok());

	assert_eq!(pool.global_fee_growth[PoolSide::Asset0], U256::MAX);
	assert_eq!(pool.global_fee_growth[!PoolSide::Asset0], U256::MAX);

	let (_, fees_owed) = pool.burn(id, MIN_TICK_UNISWAP_LOW, MAX_TICK_UNISWAP_LOW, 0).unwrap();

	// DIFF: no fees accrued
	assert_eq!(fees_owed[PoolSide::Asset0], 0u128);
	assert_eq!(fees_owed[!PoolSide::Asset0], 0);
}

///////////////////////////////////////////////////////////
///                  ADDED TESTS                       ////
///////////////////////////////////////////////////////////

// Add some more tests for fees_owed collecting

// Previous tests using mint as a poke and to collect fees.

#[test]
fn test_limit_selling_asset_0_to_asset_1_tick0thru1_mint() {
	let (mut pool, _, id) = mediumpool_initialized_zerotick();
	let mut minted_capital = None;
	pool.mint(id.clone(), 0, 120, expandto18decimals(1).as_u128(), |minted| {
		minted_capital.replace(minted);
		Ok::<(), ()>(())
	})
	.unwrap();
	let minted_capital = minted_capital.unwrap();

	assert_eq!(minted_capital[PoolSide::Asset0], U256::from_dec_str("5981737760509663").unwrap());
	assert_eq!(minted_capital[!PoolSide::Asset0], U256::from_dec_str("0").unwrap());

	// somebody takes the limit order
	assert!(pool
		.swap::<Asset1ToAsset0>(U256::from_dec_str("2000000000000000000").unwrap())
		.is_ok());

	let (_, fees_owed) = pool.mint(id.clone(), 0, 120, 1, |_| Ok::<(), ()>(())).unwrap();

	assert_eq!(fees_owed[PoolSide::Asset0], 0);
	assert_eq!(fees_owed[!PoolSide::Asset0], 18107525382602);

	let (_, fees_owed) = pool.mint(id.clone(), 0, 120, 1, |_| Ok::<(), ()>(())).unwrap();
	assert_eq!(fees_owed[PoolSide::Asset0], 0);
	assert_eq!(fees_owed[!PoolSide::Asset0], 0);

	let (burned, fees_owed) = pool.burn(id, 0, 120, expandto18decimals(1).as_u128()).unwrap();
	assert_eq!(burned[PoolSide::Asset0], U256::from_dec_str("0").unwrap());
	assert_eq!(burned[!PoolSide::Asset0], U256::from_dec_str("6017734268818165").unwrap());

	// DIFF: position fully burnt
	assert_eq!(fees_owed[PoolSide::Asset0], 0);
	assert_eq!(fees_owed[!PoolSide::Asset0], 0);

	assert!(pool.current_tick > 120)
}

#[test]
fn test_limit_selling_paior_tick1thru0_mint() {
	let (mut pool, _, id) = mediumpool_initialized_zerotick();
	let mut minted_capital = None;
	pool.mint(id.clone(), -120, 0, expandto18decimals(1).as_u128(), |minted| {
		minted_capital.replace(minted);
		Ok::<(), ()>(())
	})
	.unwrap();
	let minted_capital = minted_capital.unwrap();

	assert_eq!(minted_capital[!PoolSide::Asset0], U256::from_dec_str("5981737760509663").unwrap());
	assert_eq!(minted_capital[PoolSide::Asset0], U256::from_dec_str("0").unwrap());

	// somebody takes the limit order
	assert!(pool
		.swap::<Asset0ToAsset1>(U256::from_dec_str("2000000000000000000").unwrap())
		.is_ok());

	let (_, fees_owed) = pool.mint(id.clone(), -120, 0, 1, |_| Ok::<(), ()>(())).unwrap();

	assert_eq!(fees_owed[!PoolSide::Asset0], 0);
	assert_eq!(fees_owed[PoolSide::Asset0], 18107525382602);

	let (_, fees_owed) = pool.mint(id.clone(), -120, 0, 1, |_| Ok::<(), ()>(())).unwrap();
	assert_eq!(fees_owed[!PoolSide::Asset0], 0);
	assert_eq!(fees_owed[PoolSide::Asset0], 0);

	let (burned, fees_owed) = pool.burn(id, -120, 0, expandto18decimals(1).as_u128()).unwrap();
	assert_eq!(burned[!PoolSide::Asset0], U256::from_dec_str("0").unwrap());
	assert_eq!(burned[PoolSide::Asset0], U256::from_dec_str("6017734268818165").unwrap());

	// DIFF: position fully burnt
	assert_eq!(fees_owed[!PoolSide::Asset0], 0);
	assert_eq!(fees_owed[PoolSide::Asset0], 0);

	assert!(pool.current_tick < -120)
}
