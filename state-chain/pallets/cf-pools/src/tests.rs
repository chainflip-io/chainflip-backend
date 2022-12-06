use crate::mini_pool;
use cf_primitives::AssetAmount;

#[test]
fn funds_are_conserved() {
	const INITIAL_LIQUIDITY_0: AssetAmount = 200_000;
	const INITIAL_LIQUIDITY_1: AssetAmount = 20_000;
	const INITIAL_LIQUIDITY_TOTAL: AssetAmount = INITIAL_LIQUIDITY_0 + INITIAL_LIQUIDITY_1;
	const SWAP_AMOUNT: AssetAmount = 300;

	let mut pool = mini_pool::AmmPool::default();

	pool.add_liquidity(INITIAL_LIQUIDITY_0, INITIAL_LIQUIDITY_1);

	// Swapping one way should not create or destroy funds.
	let output = pool.swap(SWAP_AMOUNT);
	assert!(output > 0);
	assert_eq!(pool.get_liquidity().0, INITIAL_LIQUIDITY_0 + SWAP_AMOUNT);
	assert_eq!(
		pool.get_liquidity().0 + pool.get_liquidity().1 + output,
		INITIAL_LIQUIDITY_TOTAL + SWAP_AMOUNT
	);

	// Swapping the other way should not create or destroy funds.
	let output = pool.reverse_swap(output);
	assert_eq!(
		pool.get_liquidity().0 + pool.get_liquidity().1 + output,
		INITIAL_LIQUIDITY_TOTAL + SWAP_AMOUNT
	);
}
