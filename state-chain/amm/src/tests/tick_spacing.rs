use super::*;

// Medium Fee, tickSpacing = 12, 1:1 price
fn mediumpool_initialized_nomint() -> (PoolState, PoolAssetMap<AmountU256>, AccountId) {
	// fee_pips shall be one order of magnitude smaller than in the Uniswap pool (because
	// ONE_IN_HUNDREDTH_BIPS is /10)
	let pool = PoolState::new(3000, encodedprice1_1()).unwrap();
	let id: AccountId = AccountId::from([0xcf; 32]);
	let minted_amounts: PoolAssetMap<AmountU256> = Default::default();
	(pool, minted_amounts, id)
}

// DIFF: We have a tickspacing of 1, which means we will never have issues with it.
#[test]
fn test_tickspacing() {
	let (mut pool, _, id) = mediumpool_initialized_nomint();
	pool.mint(id.clone(), -6, 6, 1, |_| Ok::<(), ()>(())).unwrap();
	pool.mint(id.clone(), -12, 12, 1, |_| Ok::<(), ()>(())).unwrap();
	pool.mint(id.clone(), -144, 120, 1, |_| Ok::<(), ()>(())).unwrap();
	pool.mint(id, -144, -120, 1, |_| Ok::<(), ()>(())).unwrap();
}

#[test]
fn test_swapping_gaps_asset_1_to_asset_0() {
	let (mut pool, _, id) = mediumpool_initialized_nomint();
	pool.mint(id.clone(), 120000, 121200, 250000000000000000, |_| Ok::<(), ()>(()))
		.unwrap();
	assert!(pool.swap::<Asset1ToAsset0>(expandto18decimals(1)).is_ok());
	let (returned_capital, fees_owed) = pool.burn(id, 120000, 121200, 250000000000000000).unwrap();

	assert_eq!(returned_capital[PoolSide::Asset0], U256::from_dec_str("30027458295511").unwrap());
	assert_eq!(
		returned_capital[!PoolSide::Asset0],
		U256::from_dec_str("996999999999999999").unwrap()
	);

	assert_eq!(fees_owed[PoolSide::Asset0], 0);
	assert!(fees_owed[!PoolSide::Asset0] > 0);

	assert_eq!(pool.current_tick, 120196)
}

#[test]
fn test_swapping_gaps_asset_0_to_asset_1() {
	let (mut pool, _, id) = mediumpool_initialized_nomint();
	pool.mint(id.clone(), -121200, -120000, 250000000000000000, |_| Ok::<(), ()>(()))
		.unwrap();
	assert!(pool.swap::<Asset0ToAsset1>(expandto18decimals(1)).is_ok());
	let (returned_capital, fees_owed) =
		pool.burn(id, -121200, -120000, 250000000000000000).unwrap();

	assert_eq!(
		returned_capital[PoolSide::Asset0],
		U256::from_dec_str("996999999999999999").unwrap()
	);
	assert_eq!(returned_capital[!PoolSide::Asset0], U256::from_dec_str("30027458295511").unwrap());

	assert_eq!(fees_owed[!PoolSide::Asset0], 0);
	assert!(fees_owed[PoolSide::Asset0] > 0);

	assert_eq!(pool.current_tick, -120197)
}

#[test]
fn test_cannot_run_ticktransition_twice() {
	let id: AccountId = AccountId::from([0xcf; 32]);

	let p0 = PoolState::sqrt_price_at_tick(-24081) + 1;
	let mut pool = PoolState::new(3000, p0).unwrap();
	assert_eq!(pool.current_liquidity, 0);
	assert_eq!(pool.current_tick, -24081);

	// add a bunch of liquidity around current price
	pool.mint(id.clone(), -24082, -24080, expandto18decimals(1000).as_u128(), |_| Ok::<(), ()>(()))
		.unwrap();
	assert_eq!(pool.current_liquidity, expandto18decimals(1000).as_u128());

	pool.mint(id, -24082, -24081, expandto18decimals(1000).as_u128(), |_| Ok::<(), ()>(()))
		.unwrap();
	assert_eq!(pool.current_liquidity, expandto18decimals(1000).as_u128());

	// check the math works out to moving the price down 1, sending no amount out, and having
	// some amount remaining
	let (amount_swapped, _) =
		pool.swap::<Asset0ToAsset1>(U256::from_dec_str("3").unwrap()).unwrap();
	assert_eq!(amount_swapped, U256::from_dec_str("0").unwrap());

	assert_eq!(pool.current_tick, -24082);
	assert_eq!(pool.current_sqrt_price, p0 - 1);
	assert_eq!(pool.current_liquidity, 2000000000000000000000u128);
}
