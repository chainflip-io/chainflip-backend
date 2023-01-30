use std::panic::catch_unwind;

use super::*;
use serde::{Deserialize, Serialize};

fn format_price_f64(price: f64) -> f64 {
	(price / 2f64.powf(96f64)).powf(2f64)
}

macro_rules! assert_approx_equal_percentage {
	($a:expr, $b:expr, $margin:expr $(,)? ) => {
		// margin = 1 means 0.001%
		let margin = $margin / 10000.0;
		let max = $a.max($b);
		assert!(
			($a - $b).abs() <= (max * margin).abs(),
			"{} and {} are not within the margin of {}.",
			$a,
			$b,
			margin
		);
	};
}

macro_rules! assert_approx_equal_percentage_u256 {
	($a:expr, $b:expr, $margin:expr $(,)? ) => {
		let max = $a.max($b);
		let min = $a.min($b);
		assert!(
			max - min <= (max * $margin) / U256::from_dec_str("10000").unwrap(),
			"{} and {} are not within the margin of {}.",
			$a,
			$b,
			$margin
		);
	};
}

#[derive(Serialize, Deserialize, Debug)]
pub enum OutputFormats {
	Format(OutputFormat),
	FormatErrors(OutputFormatErrors),
}

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
pub struct OutputFormat {
	amount0Before: String,
	amount0Delta: String,
	amount1Before: String,
	amount1Delta: String,
	executionPrice: String,
	feeGrowthGlobal0X128Delta: String,
	feeGrowthGlobal1X128Delta: String,
	poolPriceAfter: String,
	poolPriceBefore: String,
	tickAfter: i32,
	tickBefore: i32,
}

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
pub struct OutputFormatErrors {
	poolBalance0: String,
	poolBalance1: String,
	poolPriceBefore: String,
	swapError: String,
	tickBefore: i32,
}

pub const MIN_TICK_LOW: Tick = -887270;
pub const MIN_TICK_MEDIUM: Tick = -887220;
pub const MIN_TICK_HIGH: Tick = -887200;
pub const MAX_TICK_LOW: Tick = -MIN_TICK_LOW;
pub const MAX_TICK_MEDIUM: Tick = -MIN_TICK_MEDIUM;
pub const MAX_TICK_HIGH: Tick = -MIN_TICK_HIGH;

struct PositionParams {
	pub lower_tick: Tick,
	pub upper_tick: Tick,
	pub liquidity: u128,
}

#[test]
fn test_swaps_with_pool_configs() {
	use serde_json::{self, Value};

	let raw_json = include_bytes!("pruned_snapshot.json");
	let json_data: Value = serde_json::from_slice(raw_json).expect("JSON was not well-formatted");

	//let expected_vec = expected_output.as_array().unwrap();
	//let des = expected_vec.iter().for_each(|value| value.deserialize_tuple_struct(name, len,
	// visitor))
	let mut expected_output: Vec<OutputFormats> = vec![];

	match json_data {
		Value::Array(arr) =>
			for value in arr {
				if let Value::Object(map) = value {
					// Workaround to detect which type it is
					if map.contains_key("amount0Before") {
						let output: OutputFormat = serde_json::from_value(Value::Object(map))
							.expect("Failed to deserialize as OutputFormat");
						expected_output.push(OutputFormats::Format(output));
					} else if map.contains_key("swapError") {
						let output: OutputFormatErrors = serde_json::from_value(Value::Object(map))
							.expect("Failed to deserialize as OutputFormatErrors");
						expected_output.push(OutputFormats::FormatErrors(output));
					} else {
						panic!("Failed to parse one of the pool's expected outputs");
					}
				}
			},
		_ => panic!("Unexpected JSON format"),
	};

	const LOW_FEE: u32 = 500;
	const MEDIUM_FEE: u32 = 3_000;
	const HIGH_FEE: u32 = 10_000;
	const LOW_TICK_SPACING: i32 = 10;
	const MEDIUM_TICK_SPACING: i32 = 60;
	const _HIGH_TICK_SPACING: i32 = 200;

	fn setup_pool(
		initial_price: AmountU256,
		fee_amount: u32,
		positions: Vec<PositionParams>,
	) -> (PoolState, PoolAssetMap<AmountU256>) {
		// encodeSqrtPrice (1,10) -> 25054144837504793118650146401
		let mut pool = PoolState::new(fee_amount, initial_price).unwrap();
		const ID: [u8; 32] = [0xcf; 32];

		let mut amounts_minted: PoolAssetMap<AmountU256> = Default::default();

		positions.iter().for_each(|position| {
			pool.mint::<()>(
				ID.into(),
				position.lower_tick,
				position.upper_tick,
				position.liquidity,
				|minted| {
					amounts_minted[PoolSide::Asset0] += minted[PoolSide::Asset0];
					amounts_minted[!PoolSide::Asset0] += minted[!PoolSide::Asset0];
					Ok(())
				},
			)
			.unwrap();
		});

		(pool, amounts_minted)
	}

	let pool_0 = setup_pool(
		encodedprice1_1(),
		LOW_FEE,
		vec![PositionParams {
			lower_tick: MIN_TICK_LOW,
			upper_tick: MAX_TICK_LOW,
			liquidity: 2_000_000_000_000_000_000,
		}],
	);
	let pool_1 = setup_pool(
		encodedprice1_1(),
		MEDIUM_FEE,
		vec![PositionParams {
			lower_tick: MIN_TICK_MEDIUM,
			upper_tick: MAX_TICK_MEDIUM,
			liquidity: 2_000_000_000_000_000_000,
		}],
	);
	let pool_2 = setup_pool(
		encodedprice1_1(),
		HIGH_FEE,
		vec![PositionParams {
			lower_tick: MIN_TICK_HIGH,
			upper_tick: MAX_TICK_HIGH,
			liquidity: 2_000_000_000_000_000_000,
		}],
	);
	let pool_3 = setup_pool(
		U256::from_dec_str("250541448375047931186413801569").unwrap(), //encodeSqrtPrice (10,1)
		MEDIUM_FEE,
		vec![PositionParams {
			lower_tick: MIN_TICK_MEDIUM,
			upper_tick: MAX_TICK_MEDIUM,
			liquidity: 2_000_000_000_000_000_000,
		}],
	);
	let pool_4 = setup_pool(
		U256::from_dec_str("25054144837504793118650146401").unwrap(), //encodeSqrtPrice (1,10)
		MEDIUM_FEE,
		vec![PositionParams {
			lower_tick: MIN_TICK_MEDIUM,
			upper_tick: MAX_TICK_MEDIUM,
			liquidity: 2_000_000_000_000_000_000,
		}],
	);
	let pool_5 = setup_pool(
		encodedprice1_1(),
		MEDIUM_FEE,
		vec![
			PositionParams {
				lower_tick: MIN_TICK_MEDIUM,
				upper_tick: -MEDIUM_TICK_SPACING,
				liquidity: 2_000_000_000_000_000_000,
			},
			PositionParams {
				lower_tick: MEDIUM_TICK_SPACING,
				upper_tick: MAX_TICK_MEDIUM,
				liquidity: 2_000_000_000_000_000_000,
			},
		],
	);
	let pool_6 = setup_pool(
		encodedprice1_1(),
		MEDIUM_FEE,
		vec![
			PositionParams {
				lower_tick: MIN_TICK_MEDIUM,
				upper_tick: MAX_TICK_MEDIUM,
				liquidity: 2_000_000_000_000_000_000,
			},
			PositionParams {
				lower_tick: MIN_TICK_MEDIUM,
				upper_tick: -MEDIUM_TICK_SPACING,
				liquidity: 2_000_000_000_000_000_000,
			},
			PositionParams {
				lower_tick: MEDIUM_TICK_SPACING,
				upper_tick: MAX_TICK_MEDIUM,
				liquidity: 2_000_000_000_000_000_000,
			},
		],
	);
	let pool_7 = setup_pool(
		encodedprice1_1(),
		LOW_FEE,
		vec![PositionParams {
			lower_tick: -LOW_TICK_SPACING,
			upper_tick: LOW_TICK_SPACING,
			liquidity: 2_000_000_000_000_000_000,
		}],
	);
	let pool_8 = setup_pool(
		encodedprice1_1(),
		MEDIUM_FEE,
		vec![PositionParams {
			lower_tick: 0,
			upper_tick: 2000 * MEDIUM_TICK_SPACING,
			liquidity: 2_000_000_000_000_000_000,
		}],
	);
	let pool_9 = setup_pool(
		encodedprice1_1(),
		MEDIUM_FEE,
		vec![PositionParams {
			lower_tick: -2000 * MEDIUM_TICK_SPACING,
			upper_tick: 0,
			liquidity: 2_000_000_000_000_000_000,
		}],
	);
	let pool_10 = setup_pool(
		U256::from_dec_str("1033437718471923701407239276819587054334136928048").unwrap(), /* encodeSqrtPrice (2**127,1) */
		MEDIUM_FEE,
		vec![PositionParams {
			lower_tick: MIN_TICK_MEDIUM,
			upper_tick: MAX_TICK_MEDIUM,
			liquidity: 2_000_000_000_000_000_000,
		}],
	);
	let pool_11 = setup_pool(
		U256::from_dec_str("6085630636").unwrap(), //encodeSqrtPrice (1,2**127)
		MEDIUM_FEE,
		vec![PositionParams {
			lower_tick: MIN_TICK_MEDIUM,
			upper_tick: MAX_TICK_MEDIUM,
			liquidity: 2_000_000_000_000_000_000,
		}],
	);
	// Removed pool 12 because the initial values can't be used in our case
	// (MAX_LIQUIDITY_PER_TICK is too big

	let pool_13 = setup_pool(
		U256::from_dec_str("1461446703485210103287273052203988822378723970341").unwrap(), /* MaxSqrtRatio - 1 */
		MEDIUM_FEE,
		vec![PositionParams {
			lower_tick: MIN_TICK_MEDIUM,
			upper_tick: MAX_TICK_MEDIUM,
			liquidity: 2_000_000_000_000_000_000,
		}],
	);
	let pool_14 = setup_pool(
		U256::from_dec_str("4295128739").unwrap(), // MinSqrtRatio
		MEDIUM_FEE,
		vec![PositionParams {
			lower_tick: MIN_TICK_MEDIUM,
			upper_tick: MAX_TICK_MEDIUM,
			liquidity: 2_000_000_000_000_000_000,
		}],
	);

	let mut output_index = 0;
	for (pool_index, (pool_initial, minted_funds)) in [
		pool_10, pool_11, pool_2, pool_13, pool_14, pool_0, pool_7, pool_5, pool_1, pool_6, pool_4,
		pool_3, pool_8, pool_9,
	]
	.into_iter()
	.enumerate()
	{
		for (swap_amount, input_side) in [
			("1000", PoolSide::Asset0),
			("1000", PoolSide::Asset1),
			("1000000000000000000", PoolSide::Asset0),
			("1000000000000000000", PoolSide::Asset1),
		] {
			let mut pool = pool_initial.clone();
			let swap_input = U256::from_dec_str(swap_amount).unwrap();
			let swap_result = match input_side {
				PoolSide::Asset0 => pool.swap_from_asset_0_to_asset_1(swap_input),
				PoolSide::Asset1 => pool.swap_from_asset_1_to_asset_0(swap_input),
			};

			let do_checks = || match &expected_output[output_index] {
				OutputFormats::Format(output) => {
					// Using assert_approx_equal_percentage to compare floats because the
					// operations return extra decimals compared to the snapshot. Margin of
					// 0.001%

					// Check initial tick
					assert_eq!(pool_initial.current_tick, output.tickBefore);
					// Check initial pool price
					let num_f64 = output.poolPriceBefore.as_str().parse::<f64>().unwrap();
					let formatted_price = format_price_f64(
						pool_initial.current_sqrt_price.to_string().parse::<f64>().unwrap(),
					);
					assert_approx_equal_percentage!(num_f64, formatted_price, 1f64);

					// Compare amounts before
					assert_approx_equal_percentage_u256!(
						minted_funds[PoolSide::Asset0],
						U256::from_dec_str(output.amount0Before.as_str()).unwrap(),
						U256::from(1),
					);
					assert_approx_equal_percentage_u256!(
						minted_funds[PoolSide::Asset1],
						U256::from_dec_str(output.amount1Before.as_str()).unwrap(),
						U256::from(1),
					);
					match swap_result {
						Ok(_) => {},
						Err(SwapError::InsufficientLiquidity) => {
							// Catch cases where the liquidity is not enough to cover the
							// swap
							match input_side {
								PoolSide::Asset0 => {
									assert!(
										output.amount0Delta != "1000" &&
											output.amount0Delta != "1000000000000000000"
									);
								},
								PoolSide::Asset1 => {
									assert!(
										output.amount1Delta != "1000" &&
											output.amount1Delta != "1000000000000000000"
									);
								},
							}
							return
						},
					}

					// Pools x swapcases combinations that differ from the result in the snapshot
					// pool 10, 11, 13 and 14. This is when the swap is done across a large range of
					// ticks, which Solidity does in multiple steps due to the Bitmap
					// implementation. For small swaps the rounding in each step is quite big which
					// causes the results to be quite different. In the case of a larger swap the
					// rounding doesn't impact the result significantly.
					if output_index == 0 ||
						output_index == 5 || output_index == 12 ||
						output_index == 17
					{
						return
					}

					// Compare tick after
					assert_eq!(pool.current_tick, output.tickAfter);

					// Compare poolPriceAfter.
					let num_f64 = output.poolPriceAfter.as_str().parse::<f64>().unwrap();
					let formatted_price = format_price_f64(
						pool.current_sqrt_price.to_string().parse::<f64>().unwrap(),
					);
					assert_approx_equal_percentage!(num_f64, formatted_price, 1f64);

					// Compare feeGrowthGlobal
					let fee_growth_global_0_snapshot =
						U256::from_dec_str(output.feeGrowthGlobal0X128Delta.as_str()).unwrap();
					let fee_growth_global_1_snapshot =
						U256::from_dec_str(output.feeGrowthGlobal1X128Delta.as_str()).unwrap();

					assert_approx_equal_percentage_u256!(
						pool.global_fee_growth[PoolSide::Asset0] -
							pool_initial.global_fee_growth[PoolSide::Asset0],
						fee_growth_global_0_snapshot,
						U256::from(1),
					);
					assert_approx_equal_percentage_u256!(
						pool.global_fee_growth[PoolSide::Asset1] -
							pool_initial.global_fee_growth[PoolSide::Asset1],
						fee_growth_global_1_snapshot,
						U256::from(1),
					);

					// No need to check executionPrice. Checking amount0Delta and amount1Delta
					// ensures executionPrice will be correct.

					// Compare amount Delta

					// Skip amount checks for swaps that emptied the pool. This is because the
					// behaviour of the pool without liquidity is different from Uniswap
					// https://www.notion.so/chainflip/Fallible-swaps-17e5104c3a204323bb271ad6c7cae2e6

					// Any swap that empties the pool will have been catched before.
					let (swap_output, _) = swap_result.unwrap();

					if input_side == PoolSide::Asset0 {
						assert!(input_side == PoolSide::Asset0);
						assert_eq!(
							output.amount0Delta.to_string().parse::<f64>().unwrap(),
							swap_input.to_string().parse::<f64>().unwrap()
						);

						assert_approx_equal_percentage!(
							output.amount1Delta.to_string().parse::<f64>().unwrap(),
							-swap_output.to_string().parse::<f64>().unwrap(),
							1f64,
						);
					} else {
						assert!(input_side == PoolSide::Asset1);
						assert_eq!(
							output.amount1Delta.to_string().parse::<f64>().unwrap(),
							swap_input.to_string().parse::<f64>().unwrap()
						);

						assert_approx_equal_percentage!(
							output.amount0Delta.to_string().parse::<f64>().unwrap(),
							-swap_output.to_string().parse::<f64>().unwrap(),
							1f64,
						);
					}
				},
				OutputFormats::FormatErrors(output) => {
					assert!(
						swap_result.is_err(),
						"Expected swap {:?} for pool[{pool_index}] {:#?} to fail but it succeeded: {:?}.",
						(swap_amount, input_side),
						pool,
						swap_result
					);
					// We don't have a sqrtPriceLimitX96 so in our case it won't fail. We still
					// check the intial values.
					assert_eq!(pool_initial.current_tick, output.tickBefore);

					assert_approx_equal_percentage_u256!(
						minted_funds[PoolSide::Asset0],
						U256::from_dec_str(output.poolBalance0.as_str()).unwrap(),
						U256::from(1),
					);
					assert_approx_equal_percentage_u256!(
						minted_funds[PoolSide::Asset1],
						U256::from_dec_str(output.poolBalance1.as_str()).unwrap(),
						U256::from(1),
					);
					let num_f64 = output.poolPriceBefore.as_str().parse::<f64>().unwrap();
					let formatted_price = format_price_f64(
						pool_initial.current_sqrt_price.to_string().parse::<f64>().unwrap(),
					);
					assert_approx_equal_percentage!(num_f64, formatted_price, 1f64);
				},
			};

			assert!(
				catch_unwind(do_checks).is_ok(),
				r#"
				Test case failed for swap {:?} for pool[{pool_index}] {:#?}
				Expected output: {:#?}
				"#,
				(swap_amount, input_side),
				pool,
				expected_output[output_index],
			);

			output_index += 1;
		}
	}
}
