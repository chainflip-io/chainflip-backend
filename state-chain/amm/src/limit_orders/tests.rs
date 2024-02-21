use crate::{
	common::{
		mul_div, sqrt_price_at_tick, tick_at_sqrt_price, MAX_SQRT_PRICE, MAX_TICK, MIN_SQRT_PRICE,
		MIN_TICK,
	},
	limit_orders, range_orders,
	test_utilities::rng_u256_inclusive_bound,
};

use super::*;

use cf_utilities::{assert_ok, assert_panics};
use rand::{Rng, SeedableRng};

type LiquidityProvider = cf_primitives::AccountId;
type PoolState = super::PoolState<LiquidityProvider>;

/// The amounts used as parameters to input_amount_floor, input_amount_ceil, output_amount_floor are
/// guaranteed to be <= MAX_FIXED_POOL_LIQUIDITY. This test checks that MAX_FIXED_POOL_LIQUIDITY is
/// set low enough that those calculations don't overflow.
#[test]
fn max_liquidity() {
	macro_rules! checks {
		($t:ty, $price:ident) => {
			<$t>::input_amount_floor(MAX_FIXED_POOL_LIQUIDITY, $price);
			<$t>::input_amount_ceil(MAX_FIXED_POOL_LIQUIDITY, $price);
			<$t>::output_amount_floor(MAX_FIXED_POOL_LIQUIDITY, $price);
		};
	}

	for price in [MIN_SQRT_PRICE, MAX_SQRT_PRICE].map(sqrt_price_to_price) {
		checks!(BaseToQuote, price);
		checks!(QuoteToBase, price);
	}
}

#[test]
fn test_float() {
	let mut rng = rand::rngs::StdRng::from_seed([8u8; 32]);

	fn rng_u256(rng: &mut impl rand::Rng) -> U256 {
		U256([(); 4].map(|()| rng.gen()))
	}

	fn rng_u256_numerator_denominator(rng: &mut impl rand::Rng) -> (U256, U256) {
		let numerator = rng_u256(rng);
		(numerator, rng_u256_inclusive_bound(rng, numerator..=U256::MAX))
	}

	for x in std::iter::repeat(()).take(16).map(|_| rng_u256(&mut rng)) {
		assert_eq!(FloatBetweenZeroAndOne::max(), FloatBetweenZeroAndOne::max().mul_div_ceil(x, x));
	}

	for ((x, y), z) in std::iter::repeat(())
		.take(16)
		.map(|_| (rng_u256_numerator_denominator(&mut rng), rng_u256(&mut rng)))
	{
		let f = FloatBetweenZeroAndOne::max().mul_div_ceil(x, y);

		assert_eq!((z, z), FloatBetweenZeroAndOne::integer_mul_div(z, &f, &f));
	}

	for ((x, y), z) in
		(0..16).map(|_| (rng_u256_numerator_denominator(&mut rng), rng_u256(&mut rng)))
	{
		let (floor, ceil) = FloatBetweenZeroAndOne::integer_mul_div(
			z,
			&FloatBetweenZeroAndOne::max().mul_div_ceil(x, y),
			&FloatBetweenZeroAndOne::max(),
		);
		let (bound_floor, bound_ceil) = mul_div(z, x, y);

		assert!(floor >= bound_floor && ceil >= bound_ceil);
	}

	for _ in 0..1024 {
		let initial_value = rng_u256(&mut rng);
		let initial_float = FloatBetweenZeroAndOne::max();

		let (final_value_floor, final_value_ceil, final_float) = (0..rng.gen_range(8..256))
			.map(|_| rng_u256_numerator_denominator(&mut rng))
			.fold(
				(initial_value, initial_value, initial_float.clone()),
				|(value_floor, value_ceil, float), (n, d)| {
					(
						mul_div_floor(value_floor, n, d),
						mul_div_ceil(value_ceil, n, d),
						float.mul_div_ceil(n, d),
					)
				},
			);

		let final_value_via_float =
			FloatBetweenZeroAndOne::integer_mul_div(initial_value, &final_float, &initial_float).0;

		assert!(final_value_ceil >= final_value_via_float);
		assert!(final_value_floor <= final_value_via_float);
	}

	{
		let low_mantissa =
			FloatBetweenZeroAndOne::max().mul_div_ceil(U256::one() << 255, U256::MAX);
		let high_mantissa = low_mantissa.mul_div_ceil(U256::MAX >> 1, U256::one() << 255);

		assert!(low_mantissa.normalised_mantissa < high_mantissa.normalised_mantissa);
		assert_eq!(
			FloatBetweenZeroAndOne::integer_mul_div(U256::MAX, &high_mantissa, &low_mantissa),
			(U256::MAX - 2, U256::MAX - 1)
		);
	}

	{
		let min_mantissa =
			FloatBetweenZeroAndOne::max().mul_div_ceil(U256::one() << 255, U256::MAX);

		assert_eq!(min_mantissa.normalised_mantissa, U256::one() << 255);

		let float = min_mantissa.mul_div_ceil(U256::one(), U256::MAX);

		assert_eq!(float.negative_exponent, U256::from(256));
		assert_eq!(float.normalised_mantissa, (U256::one() << 255) + U256::one());
	}

	{
		assert_panics!(FloatBetweenZeroAndOne::max().mul_div_ceil(2.into(), 1.into()));
		assert_panics!(
			FloatBetweenZeroAndOne::max().mul_div_ceil(U256::MAX, U256::MAX / U256::from(2))
		);
		assert_panics!(FloatBetweenZeroAndOne::max().mul_div_ceil(0.into(), 1.into()));
	}

	{
		assert_panics!(FloatBetweenZeroAndOne::integer_mul_div(
			1.into(),
			&FloatBetweenZeroAndOne::max(),
			&FloatBetweenZeroAndOne::max().mul_div_ceil(1.into(), 2.into())
		));
		assert_panics!(FloatBetweenZeroAndOne::integer_mul_div(
			U256::MAX,
			&FloatBetweenZeroAndOne::max(),
			&FloatBetweenZeroAndOne::max().mul_div_ceil(1.into(), 2.into())
		));
	}

	fn min_float() -> FloatBetweenZeroAndOne {
		FloatBetweenZeroAndOne {
			negative_exponent: U256::MAX,
			normalised_mantissa: U256::one() << 255,
		}
	}

	{
		assert_eq!(min_float(), min_float().mul_div_ceil(U256::one(), U256::from(256)));
		assert_eq!(min_float(), min_float().mul_div_ceil(U256::MAX - 1, U256::MAX));
	}

	{
		assert_eq!(
			(U256::zero(), U256::one()),
			FloatBetweenZeroAndOne::integer_mul_div(
				U256::MAX,
				&min_float(),
				&FloatBetweenZeroAndOne::max()
			)
		);
		assert_eq!(
			(U256::zero(), U256::one()),
			FloatBetweenZeroAndOne::integer_mul_div(
				U256::one(),
				&min_float(),
				&FloatBetweenZeroAndOne::max()
			)
		);
		assert_eq!(
			(U256::MAX, U256::MAX),
			FloatBetweenZeroAndOne::integer_mul_div(U256::MAX, &min_float(), &min_float())
		);
		assert_eq!(
			(U256::one() << 255, U256::one() << 255),
			FloatBetweenZeroAndOne::integer_mul_div(
				U256::MAX,
				&min_float(),
				&FloatBetweenZeroAndOne {
					normalised_mantissa: U256::MAX,
					negative_exponent: U256::MAX
				}
			)
		);
		assert_eq!(
			(U256::zero(), U256::zero()),
			FloatBetweenZeroAndOne::integer_mul_div(
				U256::zero(),
				&min_float(),
				&FloatBetweenZeroAndOne {
					normalised_mantissa: U256::MAX,
					negative_exponent: U256::MAX
				}
			)
		);
		assert_eq!(
			(U256::zero(), U256::one()),
			FloatBetweenZeroAndOne::integer_mul_div(
				U256::one(),
				&min_float(),
				&FloatBetweenZeroAndOne {
					normalised_mantissa: U256::MAX,
					negative_exponent: U256::MAX
				}
			)
		);
	}

	{
		assert!(FloatBetweenZeroAndOne::max() > min_float());
		assert!(min_float() <= min_float());
		assert!(FloatBetweenZeroAndOne::max() <= FloatBetweenZeroAndOne::max());
		assert!(
			FloatBetweenZeroAndOne { normalised_mantissa: U256::MAX, negative_exponent: U256::MAX } >
				min_float()
		);
		assert!(
			FloatBetweenZeroAndOne {
				normalised_mantissa: U256::one() << 255,
				negative_exponent: U256::MAX
			} <= min_float()
		);
		assert!(
			FloatBetweenZeroAndOne {
				normalised_mantissa: U256::one() << 255,
				negative_exponent: U256::MAX - 1
			} > min_float()
		);
	}

	{
		assert_eq!(
			FloatBetweenZeroAndOne::right_shift_mod(U512::MAX, U256::MAX),
			(U512::zero(), U512::MAX)
		);
		assert_eq!(
			FloatBetweenZeroAndOne::right_shift_mod(U512::MAX, 512.into()),
			(U512::zero(), U512::MAX)
		);
		assert_eq!(
			FloatBetweenZeroAndOne::right_shift_mod(U512::MAX, 511.into()),
			(U512::one(), U512::MAX >> 1)
		);
		assert_eq!(
			FloatBetweenZeroAndOne::right_shift_mod(U512::MAX, 256.into()),
			(U256::MAX.into(), U256::MAX.into())
		);
		assert_eq!(
			FloatBetweenZeroAndOne::right_shift_mod(U512::MAX, 255.into()),
			(U512::MAX >> 255, (U256::MAX >> 1).into())
		);
		assert_eq!(
			FloatBetweenZeroAndOne::right_shift_mod(U512::zero(), U256::MAX),
			(U512::zero(), U512::zero())
		);
		assert_eq!(
			FloatBetweenZeroAndOne::right_shift_mod(U512::zero(), 512.into()),
			(U512::zero(), U512::zero())
		);
		assert_eq!(
			FloatBetweenZeroAndOne::right_shift_mod(U512::zero(), 511.into()),
			(U512::zero(), U512::zero())
		);
		assert_eq!(
			FloatBetweenZeroAndOne::right_shift_mod(U512::zero(), 255.into()),
			(U512::zero(), U512::zero())
		);
		assert_eq!(
			FloatBetweenZeroAndOne::right_shift_mod(U512::zero(), 128.into()),
			(U512::zero(), U512::zero())
		);
	}
}

#[test]
fn fee_hundredth_pips() {
	for bad in [u32::MAX, ONE_IN_HUNDREDTH_PIPS, (ONE_IN_HUNDREDTH_PIPS / 2) + 1] {
		assert!(matches!(PoolState::new(bad), Err(NewError::InvalidFeeAmount)));
	}

	for good in [0, 1, ONE_IN_HUNDREDTH_PIPS / 2] {
		assert_ok!(PoolState::new(good));
	}
}

#[test]
fn historics() {
	{
		let lp = LiquidityProvider::from([0; 32]);
		let amount: Amount = 1000.into();
		let mut pool_state = PoolState::new(MAX_LP_FEE).unwrap();
		assert_eq!(
			assert_ok!(pool_state.collect_and_mint::<BaseToQuote>(&lp, 0, amount)),
			(
				Collected {
					fees: Amount::zero(),
					bought_amount: Amount::zero(),
					sold_amount: Amount::zero(),
					accumulative_fees: Amount::zero(),
					original_amount: Amount::zero()
				},
				PositionInfo::new(amount)
			)
		);

		let swap_amount = Amount::from(50);
		let bought_amount = Amount::from(24);
		let fees = Amount::from(24);

		pool_state.swap::<BaseToQuote>(swap_amount, None);

		assert_eq!(
			assert_ok!(pool_state.collect::<BaseToQuote>(&lp, 0)),
			(
				Collected {
					fees,
					bought_amount,
					sold_amount: bought_amount,
					accumulative_fees: 24.into(),
					original_amount: amount
				},
				PositionInfo::new(amount - Amount::from(25))
			)
		);

		pool_state.swap::<BaseToQuote>(swap_amount, None);

		assert_eq!(
			assert_ok!(pool_state.collect::<BaseToQuote>(&lp, 0)),
			(
				Collected {
					fees,
					bought_amount,
					sold_amount: bought_amount,
					accumulative_fees: 48.into(),
					original_amount: amount
				},
				PositionInfo::new(amount - Amount::from(50))
			)
		);

		pool_state.swap::<BaseToQuote>(swap_amount, None);

		assert_eq!(
			assert_ok!(pool_state.collect::<BaseToQuote>(&lp, 0)),
			(
				Collected {
					fees,
					bought_amount,
					sold_amount: bought_amount,
					accumulative_fees: 72.into(),
					original_amount: amount
				},
				PositionInfo::new(amount - Amount::from(75))
			)
		);

		assert_eq!(
			assert_ok!(pool_state.collect_and_burn::<BaseToQuote>(&lp, 0, amount)),
			(
				amount - Amount::from(75),
				Collected {
					fees: Amount::zero(),
					bought_amount: Amount::zero(),
					sold_amount: Amount::zero(),
					accumulative_fees: 72.into(),
					original_amount: amount
				},
				PositionInfo::default()
			)
		);
	}
}

#[test]
fn mint() {
	fn inner<SD: SwapDirection + limit_orders::SwapDirection + range_orders::SwapDirection>() {
		for good in [MIN_TICK, MAX_TICK] {
			let mut pool_state = PoolState::new(0).unwrap();
			assert_eq!(
				assert_ok!(pool_state.collect_and_mint::<SD>(
					&LiquidityProvider::from([0; 32]),
					good,
					1000.into()
				)),
				(Collected::default(), PositionInfo::new(1000.into()))
			);
		}

		for bad in [MIN_TICK - 1, MAX_TICK + 1] {
			let mut pool_state = PoolState::new(0).unwrap();
			assert!(matches!(
				pool_state.collect_and_mint::<SD>(
					&LiquidityProvider::from([0; 32]),
					bad,
					1000.into()
				),
				Err(PositionError::InvalidTick)
			));
		}

		for good in [MAX_FIXED_POOL_LIQUIDITY, MAX_FIXED_POOL_LIQUIDITY - 1, 1.into()] {
			let mut pool_state = PoolState::new(0).unwrap();
			assert_eq!(
				assert_ok!(pool_state.collect_and_mint::<SD>(
					&LiquidityProvider::from([0; 32]),
					0,
					good
				)),
				(Collected::default(), PositionInfo::new(good))
			);
		}

		for bad in [MAX_FIXED_POOL_LIQUIDITY + 1, MAX_FIXED_POOL_LIQUIDITY + 2] {
			let mut pool_state = PoolState::new(0).unwrap();
			assert!(matches!(
				pool_state.collect_and_mint::<SD>(&LiquidityProvider::from([0; 32]), 0, bad),
				Err(PositionError::Other(MintError::MaximumLiquidity))
			));
		}
	}

	inner::<BaseToQuote>();
	inner::<QuoteToBase>();
}

#[test]
fn burn() {
	fn inner<SD: SwapDirection + limit_orders::SwapDirection + range_orders::SwapDirection>() {
		{
			let mut pool_state = PoolState::new(0).unwrap();
			assert!(matches!(
				pool_state.collect_and_burn::<SD>(
					&LiquidityProvider::from([0; 32]),
					MIN_TICK - 1,
					1000.into()
				),
				Err(PositionError::InvalidTick)
			));
			assert!(matches!(
				pool_state.collect_and_burn::<SD>(
					&LiquidityProvider::from([0; 32]),
					MAX_TICK + 1,
					1000.into()
				),
				Err(PositionError::InvalidTick)
			));
		}
		{
			let mut pool_state = PoolState::new(0).unwrap();
			assert!(matches!(
				pool_state.collect_and_burn::<SD>(
					&LiquidityProvider::from([0; 32]),
					120,
					1000.into()
				),
				Err(PositionError::NonExistent)
			));
		}
		{
			let mut pool_state = PoolState::new(0).unwrap();
			let tick = 120;
			let amount = U256::from(1000);
			assert_eq!(
				assert_ok!(pool_state.collect_and_mint::<SD>(
					&LiquidityProvider::from([0; 32]),
					tick,
					amount
				)),
				(Collected::default(), PositionInfo::new(amount))
			);
			assert_eq!(
				assert_ok!(pool_state.collect_and_burn::<SD>(
					&LiquidityProvider::from([0; 32]),
					tick,
					amount
				)),
				(
					amount,
					Collected { original_amount: amount, ..Default::default() },
					PositionInfo::default()
				)
			);
		}
		{
			let mut pool_state = PoolState::new(0).unwrap();
			let tick = 120;
			let amount = U256::from(1000);
			assert_ok!(pool_state.collect_and_mint::<SD>(&[1u8; 32].into(), tick, 56.into()));
			assert_eq!(
				assert_ok!(pool_state.collect_and_mint::<SD>(
					&LiquidityProvider::from([0; 32]),
					tick,
					amount
				)),
				(Collected::default(), PositionInfo::new(amount))
			);
			assert_ok!(pool_state.collect_and_mint::<SD>(&[2u8; 32].into(), tick, 16.into()));
			assert_eq!(
				assert_ok!(pool_state.collect_and_burn::<SD>(
					&LiquidityProvider::from([0; 32]),
					tick,
					amount
				)),
				(
					amount,
					Collected { original_amount: amount, ..Default::default() },
					PositionInfo::default()
				)
			);
		}
		{
			let mut pool_state = PoolState::new(0).unwrap();
			let tick = 0;
			let amount = U256::from(1000);
			assert_eq!(
				assert_ok!(pool_state.collect_and_mint::<SD>(
					&LiquidityProvider::from([0; 32]),
					tick,
					amount
				)),
				(Collected::default(), PositionInfo::new(amount))
			);
			assert_eq!(pool_state.swap::<SD>(amount, None), (amount, 0.into()));
			assert_eq!(
				assert_ok!(pool_state.collect_and_burn::<SD>(
					&LiquidityProvider::from([0; 32]),
					tick,
					0.into()
				)),
				(
					0.into(),
					Collected {
						fees: 0.into(),
						sold_amount: amount,
						bought_amount: amount,
						accumulative_fees: Default::default(),
						original_amount: amount,
					},
					PositionInfo::default()
				)
			);
		}
		{
			let mut pool_state = PoolState::new(0).unwrap();
			let tick = 0;
			let amount = U256::from(1000);
			let swap = U256::from(500);
			let expected_output = U256::from(500);
			assert_eq!(
				assert_ok!(pool_state.collect_and_mint::<SD>(
					&LiquidityProvider::from([0; 32]),
					tick,
					amount
				)),
				(Collected::default(), PositionInfo::new(amount))
			);
			assert_eq!(pool_state.swap::<SD>(swap, None), (expected_output, 0.into()));
			assert_eq!(
				assert_ok!(pool_state.collect_and_burn::<SD>(
					&LiquidityProvider::from([0; 32]),
					tick,
					amount - swap
				)),
				(
					amount - swap,
					Collected {
						fees: 0.into(),
						sold_amount: swap,
						bought_amount: expected_output,
						accumulative_fees: Default::default(),
						original_amount: amount
					},
					PositionInfo::default()
				)
			);
		}
	}

	inner::<BaseToQuote>();
	inner::<QuoteToBase>();
}

#[test]
fn swap() {
	fn inner<SD: SwapDirection + limit_orders::SwapDirection + range_orders::SwapDirection>() {
		{
			let swap = U256::from(20);
			let output = swap - 1;
			{
				let mut pool_state = PoolState::new(0).unwrap();
				assert_ok!(pool_state.collect_and_mint::<SD>(
					&LiquidityProvider::from([0; 32]),
					0,
					1000.into()
				));
				assert_eq!(pool_state.swap::<SD>(swap, None), (output, 0.into()));
			}
			{
				let mut pool_state = PoolState::new(0).unwrap();
				let tick = 0;
				assert_ok!(pool_state.collect_and_mint::<SD>(
					&LiquidityProvider::from([0; 32]),
					tick,
					500.into()
				));
				assert_ok!(pool_state.collect_and_mint::<SD>(
					&LiquidityProvider::from([0; 32]),
					tick,
					500.into()
				));
				assert_eq!(pool_state.swap::<SD>(swap, None), (output, 0.into()));
			}
			{
				let mut pool_state = PoolState::new(0).unwrap();
				let tick = 0;
				assert_ok!(pool_state.collect_and_mint::<SD>(&[1u8; 32].into(), tick, 500.into()));
				assert_ok!(pool_state.collect_and_mint::<SD>(&[2u8; 32].into(), tick, 500.into()));
				assert_eq!(pool_state.swap::<SD>(swap, None), (output, 0.into()));
			}
		}
		{
			let mut pool_state = PoolState::new(100000).unwrap();
			assert_ok!(pool_state.collect_and_mint::<SD>(
				&LiquidityProvider::from([0; 32]),
				0,
				1000.into()
			));
			assert_eq!(pool_state.swap::<SD>(1000.into(), None), (900.into(), 0.into()));
		}
	}

	inner::<BaseToQuote>();
	inner::<QuoteToBase>();

	// Partial liquidity, multiple prices
	{
		let tick = 0;
		for (range, offset) in [
			(U256::from(149990000)..=U256::from(150000000), 0),
			(U256::from(150000000)..=U256::from(150010000), 1),
		] {
			let mut pool_state = PoolState::new(0).unwrap();
			assert_ok!(pool_state.collect_and_mint::<BaseToQuote>(
				&LiquidityProvider::from([0; 32]),
				tick,
				100000000.into()
			));
			assert_ok!(pool_state.collect_and_mint::<BaseToQuote>(
				&LiquidityProvider::from([0; 32]),
				offset +
					tick_at_sqrt_price(sqrt_price_at_tick(tick) * U256::from(4).integer_sqrt()),
				100000000.into()
			));
			let (output, remaining) = pool_state.swap::<BaseToQuote>(75000000.into(), None);
			assert!(range.contains(&output));
			assert_eq!(remaining, Amount::zero());
		}
	}
	{
		let tick = 0;
		for (range, offset) in [
			(U256::from(120000000)..=U256::from(120002000), 0),
			(U256::from(119998000)..=U256::from(120000000), 1),
		] {
			let mut pool_state = PoolState::new(0).unwrap();
			assert_ok!(pool_state.collect_and_mint::<QuoteToBase>(
				&LiquidityProvider::from([0; 32]),
				tick,
				100000000.into()
			));
			assert_ok!(pool_state.collect_and_mint::<QuoteToBase>(
				&LiquidityProvider::from([0; 32]),
				offset +
					tick_at_sqrt_price(sqrt_price_at_tick(tick) * U256::from(4).integer_sqrt()),
				100000000.into()
			));
			let (output, remaining) = pool_state.swap::<QuoteToBase>(180000000.into(), None);
			assert!(range.contains(&output));
			assert_eq!(remaining, Amount::zero());
		}
	}

	// All liquidity, multiple prices
	{
		let mut pool_state = PoolState::new(0).unwrap();
		let tick = 0;
		assert_ok!(pool_state.collect_and_mint::<BaseToQuote>(
			&LiquidityProvider::from([0; 32]),
			tick,
			100.into()
		));
		assert_ok!(pool_state.collect_and_mint::<BaseToQuote>(
			&LiquidityProvider::from([0; 32]),
			tick_at_sqrt_price(sqrt_price_at_tick(tick) * U256::from(4).integer_sqrt()),
			100.into()
		));
		assert_eq!(pool_state.swap::<BaseToQuote>(150.into(), None), (200.into(), 24.into()));
	}
	{
		let mut pool_state = PoolState::new(0).unwrap();
		let tick = 0;
		assert_ok!(pool_state.collect_and_mint::<QuoteToBase>(
			&LiquidityProvider::from([0; 32]),
			tick,
			100.into()
		));
		assert_ok!(pool_state.collect_and_mint::<QuoteToBase>(
			&LiquidityProvider::from([0; 32]),
			tick_at_sqrt_price(sqrt_price_at_tick(tick) * U256::from(4).integer_sqrt()),
			100.into()
		));
		assert_eq!(pool_state.swap::<QuoteToBase>(550.into(), None), (200.into(), 50.into()));
	}
}

#[cfg(feature = "slow-tests")]
#[test]
fn maximum_liquidity_swap() {
	let mut pool_state = PoolState::new(0).unwrap();

	for tick in MIN_TICK..=MAX_TICK {
		assert_eq!(
			pool_state
				.collect_and_mint::<BaseToQuote>(
					&LiquidityProvider::from([0; 32]),
					tick,
					MAX_FIXED_POOL_LIQUIDITY
				)
				.unwrap(),
			(Default::default(), PositionInfo::new(MAX_FIXED_POOL_LIQUIDITY))
		);
	}

	assert_eq!(
		MAX_FIXED_POOL_LIQUIDITY * (1 + MAX_TICK - MIN_TICK),
		std::iter::repeat_with(|| { pool_state.swap::<BaseToQuote>(Amount::MAX, None).0 })
			.take_while(|x| !x.is_zero())
			.fold(Amount::zero(), |acc, x| acc + x)
	);
}
