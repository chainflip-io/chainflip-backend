use crate::{mini_pool, mock::*, CollectedNetworkFee};
use cf_primitives::{chains::assets::any, AmmRange, AssetAmount};
use cf_traits::{LiquidityPoolApi, SwappingApi};

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

#[test]
fn funds_are_conserved_via_api() {
	const INITIAL_LIQUIDITY_0: AssetAmount = 200_000;
	const INITIAL_LIQUIDITY_1: AssetAmount = 20_000;
	const COLLECTED_NETWORK_FEE_PER_SWAP: AssetAmount = 3;
	const INITIAL_LIQUIDITY_TOTAL: AssetAmount = INITIAL_LIQUIDITY_0 + INITIAL_LIQUIDITY_1;
	const SWAP_AMOUNT: AssetAmount = 300;

	fn eth_liquidity() -> AssetAmount {
		<Pools as LiquidityPoolApi>::get_liquidity(&any::Asset::Eth).0
	}

	fn usdc_liquidity() -> AssetAmount {
		<Pools as LiquidityPoolApi>::get_liquidity(&any::Asset::Eth).1
	}

	new_test_ext().execute_with(|| {
		<Pools as LiquidityPoolApi>::deploy(
			&any::Asset::Eth,
			cf_primitives::TradingPosition::ClassicV3 {
				range: AmmRange::default(),
				volume_0: INITIAL_LIQUIDITY_0,
				volume_1: INITIAL_LIQUIDITY_1,
			},
		);

		let (output, _) =
			<Pools as SwappingApi>::swap(any::Asset::Eth, any::Asset::Usdc, SWAP_AMOUNT, 0);

		assert_eq!(CollectedNetworkFee::<Test>::get(), COLLECTED_NETWORK_FEE_PER_SWAP);

		<Pools as LiquidityPoolApi>::get_liquidity(&any::Asset::Eth);

		// Swapping one way should not create or destroy funds.
		assert!(output > 0);
		assert_eq!(eth_liquidity(), INITIAL_LIQUIDITY_0 + SWAP_AMOUNT);
		assert_eq!(
			eth_liquidity() + usdc_liquidity() + output + COLLECTED_NETWORK_FEE_PER_SWAP,
			INITIAL_LIQUIDITY_TOTAL + SWAP_AMOUNT
		);

		// Swapping the other way should not create or destroy funds.
		let (output, _) =
			<Pools as SwappingApi>::swap(any::Asset::Usdc, any::Asset::Eth, output, 0);
		assert!(output > 0);
		assert_eq!(
			eth_liquidity() + usdc_liquidity() + output + COLLECTED_NETWORK_FEE_PER_SWAP * 2,
			INITIAL_LIQUIDITY_TOTAL + SWAP_AMOUNT
		);
		assert_eq!(CollectedNetworkFee::<Test>::get(), COLLECTED_NETWORK_FEE_PER_SWAP * 2);
	});
}
