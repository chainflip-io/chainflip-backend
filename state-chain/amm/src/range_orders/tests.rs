#![cfg(test)]

use rand::{prelude::Distribution, Rng, SeedableRng};

#[cfg(feature = "slow-tests")]
use crate::common::MIN_SQRT_PRICE;
use crate::{common::Pairs, test_utilities::rng_u256_inclusive_bound};

use super::*;

type LiquidityProvider = cf_primitives::AccountId;
type PoolState = super::PoolState<LiquidityProvider>;

#[test]
fn max_liquidity() {
	// Note a tick's liquidity_delta.abs() must be less than or equal to its gross liquidity,
	// and therefore <= MAX_TICK_GROSS_LIQUIDITY Also note that the total of all tick's deltas
	// must be zero. So the maximum possible liquidity is MAX_TICK_GROSS_LIQUIDITY * ((1 +
	// MAX_TICK - MIN_TICK) / 2) The divide by 2 comes from the fact that if for example all the
	// ticks from MIN_TICK to an including -1 had deltas of MAX_TICK_GROSS_LIQUIDITY, all the
	// other tick's deltas would need to be negative or zero to satisfy the requirement that the
	// sum of all deltas is zero. Importantly this means the current_liquidity can be
	// represented as a i128 as the maximum liquidity is less than half the maximum u128
	assert!(
		MAX_TICK_GROSS_LIQUIDITY
			.checked_mul((1 + MAX_TICK - MIN_TICK) as u128 / 2)
			.unwrap() < i128::MAX as u128
	);
}

#[test]
fn r_non_zero() {
	let smallest_initial_r = 0xfffcb933bd6fad37aa2d162d1a594001u128;
	assert!(
		(smallest_initial_r.ilog2() as i32 +
			(0xfff97272373d413259a46990580e213au128.ilog2() +
				0xfff2e50f5f656932ef12357cf3c7fdccu128.ilog2() +
				0xffe5caca7e10e4e61c3624eaa0941cd0u128.ilog2() +
				0xffcb9843d60f6159c9db58835c926644u128.ilog2() +
				0xff973b41fa98c081472e6896dfb254c0u128.ilog2() +
				0xff2ea16466c96a3843ec78b326b52861u128.ilog2() +
				0xfe5dee046a99a2a811c461f1969c3053u128.ilog2() +
				0xfcbe86c7900a88aedcffc83b479aa3a4u128.ilog2() +
				0xf987a7253ac413176f2b074cf7815e54u128.ilog2() +
				0xf3392b0822b70005940c7a398e4b70f3u128.ilog2() +
				0xe7159475a2c29b7443b29c7fa6e889d9u128.ilog2() +
				0xd097f3bdfd2022b8845ad8f792aa5825u128.ilog2() +
				0xa9f746462d870fdf8a65dc1f90e061e5u128.ilog2() +
				0x70d869a156d2a1b890bb3df62baf32f7u128.ilog2() +
				0x31be135f97d08fd981231505542fcfa6u128.ilog2() +
				0x9aa508b5b7a84e1c677de54f3e99bc9u128.ilog2() +
				0x5d6af8dedb81196699c329225ee604u128.ilog2() +
				0x2216e584f5fa1ea926041bedfe98u128.ilog2() +
				0x48a170391f7dc42444e8fa2u128.ilog2()) as i32 -
			(128 * 19)) > 0
	);
}

#[test]
fn output_amounts_bounded() {
	// Note these values are significant over-estimates of the maximum output amount
	QuoteToBase::output_amount_delta_floor(
		sqrt_price_at_tick(MIN_TICK),
		sqrt_price_at_tick(MAX_TICK),
		MAX_TICK_GROSS_LIQUIDITY,
	)
	.checked_mul((1 + MAX_TICK - MIN_TICK).into())
	.unwrap();
	BaseToQuote::output_amount_delta_floor(
		sqrt_price_at_tick(MAX_TICK),
		sqrt_price_at_tick(MIN_TICK),
		MAX_TICK_GROSS_LIQUIDITY,
	)
	.checked_mul((1 + MAX_TICK - MIN_TICK).into())
	.unwrap();
}

#[cfg(feature = "slow-tests")]
#[test]
fn maximum_liquidity_swap() {
	use crate::range_orders::Size;

	let mut pool_state = PoolState::new(0, MIN_SQRT_PRICE).unwrap();

	let minted_amounts: PoolPairsMap<Amount> = (MIN_TICK..0)
		.map(|lower_tick| (lower_tick, -lower_tick))
		.map(|(lower_tick, upper_tick)| {
			pool_state
				.collect_and_mint(
					&LiquidityProvider::from([0; 32]),
					lower_tick,
					upper_tick,
					Size::Liquidity { liquidity: MAX_TICK_GROSS_LIQUIDITY },
					Result::<_, Infallible>::Ok,
				)
				.unwrap()
				.0
		})
		.fold(Default::default(), |acc, x| acc + x);

	let (output, _remaining) = pool_state.swap::<QuoteToBase>(Amount::MAX, None);

	assert!(((minted_amounts[Pairs::Base] - (MAX_TICK - MIN_TICK) /* Maximum rounding down by one per swap iteration */)..minted_amounts[Pairs::Base]).contains(&output));
}

#[test]
fn test_amounts_to_liquidity() {
	fn rng_tick_range(rng: &mut impl rand::Rng) -> (Tick, Tick) {
		let tick = rand::distributions::Uniform::new_inclusive(MIN_TICK, MAX_TICK).sample(rng);

		let upper_range = tick + 1..MAX_TICK + 1;
		let low_range = MIN_TICK..tick;

		if !upper_range.is_empty() {
			(tick, rng.gen_range(upper_range))
		} else {
			assert!(!low_range.is_empty());

			(rng.gen_range(low_range), tick)
		}
	}

	std::thread::scope(|scope| {
		for i in 0..1 {
			scope.spawn(move || {
				let mut rng: rand::rngs::StdRng = rand::rngs::StdRng::from_seed([i; 32]);

				// Iterations have been decreased to ensure tests run in a reasonable time, but
				// this has been run 100 billion times
				for _i in 0..1000000 {
					let tick = rng.gen_range(MIN_TICK..MAX_TICK);

					let pool_state = PoolState::new(
						0,
						rng_u256_inclusive_bound(
							&mut rng,
							if tick > MIN_TICK {
								sqrt_price_at_tick(tick - 1)..=sqrt_price_at_tick(tick)
							} else {
								sqrt_price_at_tick(tick)..=sqrt_price_at_tick(tick + 1)
							},
						),
					)
					.unwrap();

					let (lower, upper) = rng_tick_range(&mut rng);

					let original_liquidity =
						rand::distributions::Uniform::new_inclusive(0, MAX_TICK_GROSS_LIQUIDITY)
							.sample(&mut rng);

					let amounts = pool_state
						.inner_liquidity_to_amounts::<false>(original_liquidity, lower, upper)
						.0;

					let resultant_liquidity =
						pool_state.inner_amounts_to_liquidity(lower, upper, amounts);

					let maximum_error_from_rounding_amount =
						[amounts[Pairs::Base], amounts[Pairs::Quote]]
							.into_iter()
							.filter(|amount| !amount.is_zero())
							.map(|amount| {
								1f64 / (2f64.powf((256f64 - amount.leading_zeros() as f64) - 1f64))
							})
							.fold(0f64, f64::max);

					let maximum_error_from_rounding_liquidity = 1f64 / original_liquidity as f64;

					let error = u128::rem_euclid(
						u128::abs_diff(resultant_liquidity, original_liquidity),
						original_liquidity,
					) as f64 / (original_liquidity as f64);

					assert!(
						error <=
							maximum_error_from_rounding_amount +
								maximum_error_from_rounding_liquidity,
					);
				}
			});
		}
	});
}
