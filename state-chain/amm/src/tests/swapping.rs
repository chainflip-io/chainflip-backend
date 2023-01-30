use super::*;
use serde::{Deserialize, Serialize};

fn format_price_f64(price: f64) -> f64 {
	(price / 2f64.powf(96f64)).powf(2f64)
}

fn assert_approx_equal_percentage(a: f64, b: f64, margin: f64) {
	// margin = 1 means 0.001%
	let margin = margin / 10000.0;
	let max = a.max(b);
	assert!((a - b).abs() <= (max * margin).abs());
}

// Compare two U256 and check that they are equal within a margin of 0.001%
fn assert_approx_equal_percentage_u256(a: U256, b: U256, margin: U256) {
	let max = a.max(b);
	let min = a.min(b);
	assert!(max - min <= (max * margin) / U256::from_dec_str("10000").unwrap());
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

#[derive(Clone)]
struct PoolConfig {
	pub fee_amount: u32,
	pub tick_spacing: i32,
}

#[derive(Copy, Clone, Debug, PartialEq, PartialOrd, Eq, Ord)]
enum PoolType {
	Low,
	Medium,
	High,
}

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

	let pool_configs = BTreeMap::<PoolType, PoolConfig>::from_iter([
		(PoolType::Low, PoolConfig { fee_amount: 500, tick_spacing: 10 }),
		(PoolType::Medium, PoolConfig { fee_amount: 3000, tick_spacing: 60 }),
		(PoolType::High, PoolConfig { fee_amount: 10000, tick_spacing: 200 }),
	]);

	fn setup_pool(
		initial_price: &str,
		fee_amount: u32,
		positions: Vec<PositionParams>,
	) -> (PoolState, PoolAssetMap<AmountU256>) {
		// encodeSqrtPrice (1,10) -> 25054144837504793118650146401
		let mut pool =
			PoolState::new(fee_amount / 10, U256::from_dec_str(initial_price).unwrap()).unwrap();
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
		"79228162514264337593543950336", //encodeSqrtPrice (1,1)
		pool_configs[&PoolType::Low].clone().fee_amount,
		vec![PositionParams {
			lower_tick: MIN_TICK_LOW,
			upper_tick: MAX_TICK_LOW,
			liquidity: 2_000_000_000_000_000_000,
		}],
	);
	let pool_1 = setup_pool(
		"79228162514264337593543950336", //encodeSqrtPrice (1,1)
		pool_configs[&PoolType::Medium].clone().fee_amount,
		vec![PositionParams {
			lower_tick: MIN_TICK_MEDIUM,
			upper_tick: MAX_TICK_MEDIUM,
			liquidity: 2_000_000_000_000_000_000,
		}],
	);
	let pool_2 = setup_pool(
		"79228162514264337593543950336", //encodeSqrtPrice (1,1)
		pool_configs[&PoolType::High].clone().fee_amount,
		vec![PositionParams {
			lower_tick: MIN_TICK_HIGH,
			upper_tick: MAX_TICK_HIGH,
			liquidity: 2_000_000_000_000_000_000,
		}],
	);
	let pool_3 = setup_pool(
		"250541448375047931186413801569", //encodeSqrtPrice (10,1)
		pool_configs[&PoolType::Medium].clone().fee_amount,
		vec![PositionParams {
			lower_tick: MIN_TICK_MEDIUM,
			upper_tick: MAX_TICK_MEDIUM,
			liquidity: 2_000_000_000_000_000_000,
		}],
	);
	let pool_4 = setup_pool(
		"25054144837504793118650146401", //encodeSqrtPrice (1,10)
		pool_configs[&PoolType::Medium].clone().fee_amount,
		vec![PositionParams {
			lower_tick: MIN_TICK_MEDIUM,
			upper_tick: MAX_TICK_MEDIUM,
			liquidity: 2_000_000_000_000_000_000,
		}],
	);
	let pool_5 = setup_pool(
		"79228162514264337593543950336", //encodeSqrtPrice (1,1)
		pool_configs[&PoolType::Medium].clone().fee_amount,
		vec![
			PositionParams {
				lower_tick: MIN_TICK_MEDIUM,
				upper_tick: -pool_configs[&PoolType::Medium].tick_spacing,
				liquidity: 2_000_000_000_000_000_000,
			},
			PositionParams {
				lower_tick: pool_configs[&PoolType::Medium].tick_spacing,
				upper_tick: MAX_TICK_MEDIUM,
				liquidity: 2_000_000_000_000_000_000,
			},
		],
	);
	let pool_6 = setup_pool(
		"79228162514264337593543950336", //encodeSqrtPrice (1,1)
		pool_configs[&PoolType::Medium].clone().fee_amount,
		vec![
			PositionParams {
				lower_tick: MIN_TICK_MEDIUM,
				upper_tick: MAX_TICK_MEDIUM,
				liquidity: 2_000_000_000_000_000_000,
			},
			PositionParams {
				lower_tick: MIN_TICK_MEDIUM,
				upper_tick: -pool_configs[&PoolType::Medium].tick_spacing,
				liquidity: 2_000_000_000_000_000_000,
			},
			PositionParams {
				lower_tick: pool_configs[&PoolType::Medium].tick_spacing,
				upper_tick: MAX_TICK_MEDIUM,
				liquidity: 2_000_000_000_000_000_000,
			},
		],
	);
	let pool_7 = setup_pool(
		"79228162514264337593543950336", //encodeSqrtPrice (1,1)
		pool_configs[&PoolType::Low].clone().fee_amount,
		vec![PositionParams {
			lower_tick: -pool_configs[&PoolType::Low].tick_spacing,
			upper_tick: pool_configs[&PoolType::Low].tick_spacing,
			liquidity: 2_000_000_000_000_000_000,
		}],
	);
	let pool_8 = setup_pool(
		"79228162514264337593543950336", //encodeSqrtPrice (1,1)
		pool_configs[&PoolType::Medium].clone().fee_amount,
		vec![PositionParams {
			lower_tick: 0,
			upper_tick: 2000 * pool_configs[&PoolType::Medium].tick_spacing,
			liquidity: 2_000_000_000_000_000_000,
		}],
	);
	let pool_9 = setup_pool(
		"79228162514264337593543950336", //encodeSqrtPrice (1,1)
		pool_configs[&PoolType::Medium].clone().fee_amount,
		vec![PositionParams {
			lower_tick: -2000 * pool_configs[&PoolType::Medium].tick_spacing,
			upper_tick: 0,
			liquidity: 2_000_000_000_000_000_000,
		}],
	);
	let pool_10 = setup_pool(
		"1033437718471923701407239276819587054334136928048", //encodeSqrtPrice (2**127,1)
		pool_configs[&PoolType::Medium].clone().fee_amount,
		vec![PositionParams {
			lower_tick: MIN_TICK_MEDIUM,
			upper_tick: MAX_TICK_MEDIUM,
			liquidity: 2_000_000_000_000_000_000,
		}],
	);
	let pool_11 = setup_pool(
		"6085630636", //encodeSqrtPrice (1,2**127)
		pool_configs[&PoolType::Medium].clone().fee_amount,
		vec![PositionParams {
			lower_tick: MIN_TICK_MEDIUM,
			upper_tick: MAX_TICK_MEDIUM,
			liquidity: 2_000_000_000_000_000_000,
		}],
	);
	// Removed pool 12 because the initial values can't be used in our case
	// (MAX_LIQUIDITY_PER_TICK is too big

	let pool_13 = setup_pool(
		"1461446703485210103287273052203988822378723970341", // MaxSqrtRatio - 1
		pool_configs[&PoolType::Medium].clone().fee_amount,
		vec![PositionParams {
			lower_tick: MIN_TICK_MEDIUM,
			upper_tick: MAX_TICK_MEDIUM,
			liquidity: 2_000_000_000_000_000_000,
		}],
	);
	let pool_14 = setup_pool(
		"4295128739", // MinSqrtRatio
		pool_configs[&PoolType::Medium].clone().fee_amount,
		vec![PositionParams {
			lower_tick: MIN_TICK_MEDIUM,
			upper_tick: MAX_TICK_MEDIUM,
			liquidity: 2_000_000_000_000_000_000,
		}],
	);

	let pools = vec![
		pool_10, pool_11, pool_2, pool_13, pool_14, pool_0, pool_7, pool_5, pool_1, pool_6, pool_4,
		pool_3, pool_8, pool_9,
	];

	let pools_after = pools
		.iter()
		.map(|(pool, amount_before)| {
			// test number 0 (according to order in the snapshots file)
			let mut pool_after_swap_test_0 = pool.clone();
			let amount_swap_in_test_0 = vec![U256::from_dec_str("1000").unwrap(), U256::from(0)];
			let (amount_out_swap_test_0, _) = pool_after_swap_test_0
				.swap_from_asset_0_to_asset_1(U256::from_dec_str("1000").unwrap())
				.unwrap();
			let amount_swap_out_test_0 = vec![U256::from(0), amount_out_swap_test_0];

			// test number 1 (according to order in the snapshots file)
			let mut pool_after_swap_test_1 = pool.clone();
			let amount_swap_in_test_1 = vec![U256::from(0), U256::from_dec_str("1000").unwrap()];
			let (amount_out_swap_test_1, _) = pool_after_swap_test_1
				.swap_from_asset_1_to_asset_0(U256::from_dec_str("1000").unwrap())
				.unwrap();
			let amount_swap_out_test_1 = vec![amount_out_swap_test_1, U256::from(0)];

			// test number 2 (according to order in the snapshots file)
			let mut pool_after_swap_test_2 = pool.clone();
			let amount_swap_in_test_2 =
				vec![U256::from_dec_str("1000000000000000000").unwrap(), U256::from(0)];
			let (amount_out_swap_test_2, _) = pool_after_swap_test_2
				.swap_from_asset_0_to_asset_1(U256::from_dec_str("1000000000000000000").unwrap())
				.unwrap();
			let amount_swap_out_test_2 = vec![U256::from(0), amount_out_swap_test_2];

			// test number 3 (according to order in the snapshots file)
			let mut pool_after_swap_test_3 = pool.clone();
			let amount_swap_in_test_3 =
				vec![U256::from(0), U256::from_dec_str("1000000000000000000").unwrap()];
			let (amount_out_swap_test_3, _) = pool_after_swap_test_3
				.swap_from_asset_1_to_asset_0(U256::from_dec_str("1000000000000000000").unwrap())
				.unwrap();
			let amount_swap_out_test_3 = vec![amount_out_swap_test_3, U256::from(0)];
			vec![
				(
					pool.clone(),
					pool_after_swap_test_0,
					amount_before,
					amount_swap_in_test_0,
					amount_swap_out_test_0,
				),
				(
					pool.clone(),
					pool_after_swap_test_1,
					amount_before,
					amount_swap_in_test_1,
					amount_swap_out_test_1,
				),
				(
					pool.clone(),
					pool_after_swap_test_2,
					amount_before,
					amount_swap_in_test_2,
					amount_swap_out_test_2,
				),
				(
					pool.clone(),
					pool_after_swap_test_3,
					amount_before,
					amount_swap_in_test_3,
					amount_swap_out_test_3,
				),
			]
		})
		.collect::<Vec<_>>();

	// Check pool results
	let mut i = 0;
	for pool_vec in pools_after {
		for (pool_before, pool_after, amountbefore, amount_swap_in, amount_swap_out) in pool_vec {
			// Pools x swapcases combinations that differ from the result in the snapshot
			// pool 10, 11, 13 and 14. This is when the swap is done across a large range of
			// ticks, which Solidity does in multiple steps due to the Bitmap implementation.
			// For small swaps the rounding in each step is quite big which causes the results
			// to be quite different. In the case of a larger swap the rounding doesn't impact
			// the result significantly.
			if i == 0 || i == 5 || i == 12 || i == 17 {
				i += 1;
				continue
			}

			match &expected_output[i] {
				OutputFormats::Format(output) => {
					// Compare tick before and tick after
					assert_eq!(pool_before.current_tick, output.tickBefore);
					assert_eq!(pool_after.current_tick, output.tickAfter);

					// Using assert_approx_equal_percentage to compare floats because the
					// operations return extra decimals compared to the snapshot. Margin of
					// 0.001%

					// Compare poolPriceBefore.
					let num_f64 = output.poolPriceBefore.as_str().parse::<f64>().unwrap();
					let formatted_price = format_price_f64(
						pool_before.current_sqrt_price.to_string().parse::<f64>().unwrap(),
					);
					assert_approx_equal_percentage(num_f64, formatted_price, 1f64);

					// Compare poolPriceAfter.
					let num_f64 = output.poolPriceAfter.as_str().parse::<f64>().unwrap();
					let formatted_price = format_price_f64(
						pool_after.current_sqrt_price.to_string().parse::<f64>().unwrap(),
					);
					assert_approx_equal_percentage(num_f64, formatted_price, 1f64);

					// Compare feeGrowthGlobal
					let fee_growth_global_0_snapshot =
						U256::from_dec_str(output.feeGrowthGlobal0X128Delta.as_str()).unwrap();
					let fee_growth_global_1_snapshot =
						U256::from_dec_str(output.feeGrowthGlobal1X128Delta.as_str()).unwrap();

					assert_approx_equal_percentage_u256(
						pool_after.global_fee_growth[PoolSide::Asset0] -
							pool_before.global_fee_growth[PoolSide::Asset0],
						fee_growth_global_0_snapshot,
						U256::from(1),
					);
					assert_approx_equal_percentage_u256(
						pool_after.global_fee_growth[PoolSide::Asset1] -
							pool_before.global_fee_growth[PoolSide::Asset1],
						fee_growth_global_1_snapshot,
						U256::from(1),
					);

					// Compare amount before
					assert_approx_equal_percentage_u256(
						amountbefore[PoolSide::Asset0],
						U256::from_dec_str(output.amount0Before.as_str()).unwrap(),
						U256::from(1),
					);
					assert_approx_equal_percentage_u256(
						amountbefore[PoolSide::Asset1],
						U256::from_dec_str(output.amount1Before.as_str()).unwrap(),
						U256::from(1),
					);

					// No need to check executionPrice. Checking amount0Delta and amount1Delta
					// ensures executionPrice will be correct.

					// Compare amount Delta

					// Skip amount checks for swaps that emptied the pool. This is because the
					// behaviour of the pool without liquidity is different from Uniswap
					// https://www.notion.so/chainflip/Fallible-swaps-17e5104c3a204323bb271ad6c7cae2e6

					if output.executionPrice != "NaN" {
						// Workaround for swaps that empty the pool that amountIn will be too
						// much.

						// The two testcase swaps are exactIn. Therefore, if the amountIn is not
						// one of those values, then it's a swap that emptied the pool.	If
						// output.amount0Delta > 0 (asset0to1) and (output.amount0Delta != 1000
						// or 10000000) then skip the check for amountIn.

						if output.amount0Delta.to_string().parse::<f64>().unwrap() <= 0.0 ||
							output.amount0Delta == "1000" ||
							output.amount0Delta == "1000000000000000000"
						{
							assert_approx_equal_percentage(
								output.amount0Delta.to_string().parse::<f64>().unwrap(),
								amount_swap_in[0].to_string().parse::<f64>().unwrap() -
									amount_swap_out[0].to_string().parse::<f64>().unwrap(),
								1f64,
							);
						}

						if output.amount1Delta.to_string().parse::<f64>().unwrap() <= 0.0 ||
							output.amount1Delta == "1000" ||
							output.amount1Delta == "1000000000000000000"
						{
							assert_approx_equal_percentage(
								output.amount1Delta.to_string().parse::<f64>().unwrap(),
								amount_swap_in[1].to_string().parse::<f64>().unwrap() -
									amount_swap_out[1].to_string().parse::<f64>().unwrap(),
								10f64,
							);
						}
					}
				},
				OutputFormats::FormatErrors(output) => {
					// We don't have a sqrtPriceLimitX96 so in our case it won't fail. We still
					// check the intial values.
					assert_eq!(pool_before.current_tick, output.tickBefore);

					assert_approx_equal_percentage_u256(
						amountbefore[PoolSide::Asset0],
						U256::from_dec_str(output.poolBalance0.as_str()).unwrap(),
						U256::from(1),
					);
					assert_approx_equal_percentage_u256(
						amountbefore[PoolSide::Asset1],
						U256::from_dec_str(output.poolBalance1.as_str()).unwrap(),
						U256::from(1),
					);
					let num_f64 = output.poolPriceBefore.as_str().parse::<f64>().unwrap();
					let formatted_price = format_price_f64(
						pool_before.current_sqrt_price.to_string().parse::<f64>().unwrap(),
					);
					assert_approx_equal_percentage(num_f64, formatted_price, 1f64);
				},
			}
			// Increase expected_output index
			i += 1;
		}
	}
}
