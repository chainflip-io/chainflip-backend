//! This code implements Uniswap's price calculation logic:
//! https://github.com/Uniswap/v3-core/blob/main/contracts/UniswapV3Pool.sol
//! https://uniswap.org/whitepaper-v3.pdf
//!
//! I've removed un-needed features such as flashing, the oracle, and exactOut to ensure the code is
//! simple and easier to verify, as unlike Uniswap's contracts we cannot rely on the EVM's reverting
//! behaviour guaranteeing no state changes are made when an exception/panic occurs.
//!
//! Also I've made a few minor changes to Uniswap's maths these are all commented and marked with
//! `DIFF`.
//!
//! Other the above exceptions the results produced from this code should exactly match Uniswap's
//! numbers, for a Uniswap pool with a tick spacing of 1.
//!
//! Note: There are a few yet to be proved safe maths operations, there are marked below with `TODO:
//! Prove`. We should resolve these issues before using this code in release.

use std::{collections::BTreeMap, u128};

use enum_map::Enum;
use primitive_types::{H256, U256, U512};

pub type Tick = i32;
pub type Liquidity = u128;
pub type LiquidityProvider = H256;
pub type Amount = U256;
type SqrtPriceQ64F96 = U256;
type FeeGrowthQ128F128 = U256;

/// The minimum tick that may be passed to `sqrt_price_at_tick` computed from log base 1.0001 of
/// 2**-128
pub const MIN_TICK: Tick = -887272;
/// The maximum tick that may be passed to `sqrt_price_at_tick` computed from log base 1.0001 of
/// 2**128
pub const MAX_TICK: Tick = -MIN_TICK;

/// The minimum value that can be returned from `sqrt_price_at_tick`. Equivalent to
/// `sqrt_price_at_tick(MIN_TICK)`
const MIN_SQRT_PRICE: SqrtPriceQ64F96 = U256([0x1000276a3u64, 0x0, 0x0, 0x0]);
/// The maximum value that can be returned from `sqrt_price_at_tick`. Equivalent to
/// `sqrt_price_at_tick(MAX_TICK)`.
const MAX_SQRT_PRICE: SqrtPriceQ64F96 =
	U256([0x5d951d5263988d26u64, 0xefd1fc6a50648849u64, 0xfffd8963u64, 0x0u64]);

const MAX_TICK_GROSS_LIQUIDITY: Liquidity = Liquidity::MAX / ((1 + MAX_TICK - MIN_TICK) as u128);

const ONE_IN_PIPS: u32 = 100000;

#[derive(Clone)]
struct Position {
	liquidity: Liquidity,
	last_fee_growth_inside: enum_map::EnumMap<Ticker, FeeGrowthQ128F128>,
	fees_owed: enum_map::EnumMap<Ticker, u128>,
}
impl Position {
	fn update_fees_owed(
		&mut self,
		pool_state: &PoolState,
		lower_tick: Tick,
		lower_info: &TickInfo,
		upper_tick: Tick,
		upper_info: &TickInfo,
	) {
		let fee_growth_inside = enum_map::EnumMap::default().map(|ticker, ()| {
			let fee_growth_below = if pool_state.current_tick < lower_tick {
				pool_state.global_fee_growth[ticker] - lower_info.fee_growth_outside[ticker]
			} else {
				lower_info.fee_growth_outside[ticker]
			};

			let fee_growth_above = if pool_state.current_tick < upper_tick {
				upper_info.fee_growth_outside[ticker]
			} else {
				pool_state.global_fee_growth[ticker] - upper_info.fee_growth_outside[ticker]
			};

			pool_state.global_fee_growth[ticker] - fee_growth_below - fee_growth_above
		});
		self.fees_owed = enum_map::EnumMap::default().map(|ticker, ()| {
			// DIFF: This behaviour is different than Uniswap's. We saturate fees_owed instead of
			// overflowing

			/*
				Proof that `mul_div` does not overflow:
				Note position.liqiudity: u128
				U512::one() << 128 > u128::MAX
			*/
			let fees_owed: u128 = mul_div_floor(
				fee_growth_inside[ticker] - self.last_fee_growth_inside[ticker],
				self.liquidity.into(),
				U512::one() << 128,
			)
			.try_into()
			.unwrap_or(u128::MAX);

			// saturating is acceptable, it is on LPs to withdraw fees before you hit u128::MAX fees
			self.fees_owed[ticker].saturating_add(fees_owed)
		});
		self.last_fee_growth_inside = fee_growth_inside;
	}

	fn set_liquidity(
		&mut self,
		pool_state: &PoolState,
		new_liquidity: Liquidity,
		lower_tick: Tick,
		lower_info: &TickInfo,
		upper_tick: Tick,
		upper_info: &TickInfo,
	) {
		self.update_fees_owed(pool_state, lower_tick, lower_info, upper_tick, upper_info);
		self.liquidity = new_liquidity;
	}
}

#[derive(Clone)]
struct TickInfo {
	liquidity_delta: i128,
	liquidity_gross: u128,
	fee_growth_outside: enum_map::EnumMap<Ticker, FeeGrowthQ128F128>,
}

pub struct PoolState {
	fee_pips: u32,
	current_sqrt_price: SqrtPriceQ64F96,
	current_tick: Tick,
	current_liquidity: Liquidity,
	global_fee_growth: enum_map::EnumMap<Ticker, FeeGrowthQ128F128>,
	liquidity_map: BTreeMap<Tick, TickInfo>,
	positions: BTreeMap<(LiquidityProvider, Tick, Tick), Position>,
}

#[derive(Enum, Clone, Copy)]
pub enum Ticker {
	Base,
	Pair,
}

impl std::ops::Not for Ticker {
	type Output = Self;

	fn not(self) -> Self::Output {
		match self {
			Ticker::Base => Ticker::Pair,
			Ticker::Pair => Ticker::Base,
		}
	}
}

fn mul_div_floor<C: Into<U512>>(a: U256, b: U256, c: C) -> U256 {
	let c: U512 = c.into();
	(U256::full_mul(a, b) / c).try_into().unwrap()
}

fn mul_div_ceil<C: Into<U512>>(a: U256, b: U256, c: C) -> U256 {
	let c: U512 = c.into();

	let (d, m) = U512::div_mod(U256::full_mul(a, b), c);

	if m > U512::from(0) {
		// cannot overflow as for m > 0, c must be > 1, and as (a*b) <= U512::MAX, therefore a*b/c <
		// U512::MAX
		d + 1
	} else {
		d
	}
	.try_into()
	.unwrap()
}

trait SwapDirection {
	const INPUT_TICKER: Ticker;

	/// The xy=k maths only works while the liquidity is constant, so this function returns the
	/// closest (to the current) next tick/price where liquidity possibly changes. Note the
	/// direction of `next` is implied by the swapping direction.
	fn target_tick(
		tick: Tick,
		liquidity_map: &mut BTreeMap<Tick, TickInfo>,
	) -> Option<(&Tick, &mut TickInfo)>;

	/// Calculates the swap input amount needed to move the current price given the specified amount
	/// of liquidity
	fn input_amount_delta_ceil(
		from: SqrtPriceQ64F96,
		to: SqrtPriceQ64F96,
		liquidity: Liquidity,
	) -> Amount;
	/// Calculates the swap output amount needed to move the current price given the specified
	/// amount of liquidity
	fn output_amount_delta_floor(
		from: SqrtPriceQ64F96,
		to: SqrtPriceQ64F96,
		liquidity: Liquidity,
	) -> Amount;

	/// Calculates where the current price will be after a swap of amount given the current price
	/// and a specific amount of liquidity
	fn next_sqrt_price_from_input_amount(
		sqrt_ratio_current: SqrtPriceQ64F96,
		liquidity: Liquidity,
		amount: Amount,
	) -> SqrtPriceQ64F96;

	/// For a given tick calculates the change in current liquidity when that tick is crossed
	fn liquidity_delta_on_crossing_tick(tick_liquidity: &TickInfo) -> i128;

	/// The current tick is always the closest tick less than the current_sqrt_price
	fn current_tick_after_crossing_target_tick(target_tick: Tick) -> Tick;
}

pub struct BaseToPair {}
impl SwapDirection for BaseToPair {
	const INPUT_TICKER: Ticker = Ticker::Base;

	fn target_tick(
		current_tick: Tick,
		liquidity_map: &mut BTreeMap<Tick, TickInfo>,
	) -> Option<(&Tick, &mut TickInfo)> {
		assert!(liquidity_map.contains_key(&MIN_TICK));
		if current_tick >= MIN_TICK {
			Some(liquidity_map.range_mut(..=current_tick).next_back().unwrap())
		} else {
			assert_eq!(current_tick, Self::current_tick_after_crossing_target_tick(MIN_TICK));
			None
		}
	}

	fn input_amount_delta_ceil(
		current: SqrtPriceQ64F96,
		target: SqrtPriceQ64F96,
		liquidity: Liquidity,
	) -> Amount {
		PoolState::base_amount_delta_ceil(target, current, liquidity)
	}

	fn output_amount_delta_floor(
		current: SqrtPriceQ64F96,
		target: SqrtPriceQ64F96,
		liquidity: Liquidity,
	) -> Amount {
		PoolState::pair_amount_delta_floor(target, current, liquidity)
	}

	fn next_sqrt_price_from_input_amount(
		sqrt_ratio_current: SqrtPriceQ64F96,
		liquidity: Liquidity,
		amount: Amount,
	) -> SqrtPriceQ64F96 {
		PoolState::next_sqrt_price_from_base_input(sqrt_ratio_current, liquidity, amount)
	}

	fn liquidity_delta_on_crossing_tick(tick_liquidity: &TickInfo) -> i128 {
		-tick_liquidity.liquidity_delta
	}

	fn current_tick_after_crossing_target_tick(target_tick: Tick) -> Tick {
		target_tick - 1
	}
}

pub struct PairToBase {}
impl SwapDirection for PairToBase {
	const INPUT_TICKER: Ticker = Ticker::Pair;

	fn target_tick(
		current_tick: Tick,
		liquidity_map: &mut BTreeMap<Tick, TickInfo>,
	) -> Option<(&Tick, &mut TickInfo)> {
		assert!(liquidity_map.contains_key(&MAX_TICK));
		if current_tick < MAX_TICK {
			Some(liquidity_map.range_mut(current_tick + 1..).next().unwrap())
		} else {
			assert_eq!(current_tick, Self::current_tick_after_crossing_target_tick(MAX_TICK));
			None
		}
	}

	fn input_amount_delta_ceil(
		current: SqrtPriceQ64F96,
		target: SqrtPriceQ64F96,
		liquidity: Liquidity,
	) -> Amount {
		PoolState::pair_amount_delta_ceil(current, target, liquidity)
	}

	fn output_amount_delta_floor(
		current: SqrtPriceQ64F96,
		target: SqrtPriceQ64F96,
		liquidity: Liquidity,
	) -> Amount {
		PoolState::base_amount_delta_floor(current, target, liquidity)
	}

	fn next_sqrt_price_from_input_amount(
		sqrt_ratio_current: SqrtPriceQ64F96,
		liquidity: Liquidity,
		amount: Amount,
	) -> SqrtPriceQ64F96 {
		PoolState::next_sqrt_price_from_pair_input(sqrt_ratio_current, liquidity, amount)
	}

	fn liquidity_delta_on_crossing_tick(tick_liquidity: &TickInfo) -> i128 {
		tick_liquidity.liquidity_delta
	}

	fn current_tick_after_crossing_target_tick(target_tick: Tick) -> Tick {
		target_tick
	}
}

pub enum MintError {
	/// Invalid Tick range
	InvalidTickRange,
	/// One of the start/end ticks of the range reached its maximm gross liquidity
	MaximumGrossLiquidity,
}

pub enum PositionError<T> {
	/// Position referenced does not exist
	NonExistent,
	Other(T),
}

pub enum BurnError {
	/// Position referenced does not contain the requested liquidity
	PositionLacksLiquidity,
}

pub enum CollectError {}

impl PoolState {
	/// Creates a new pool with the specified fee and initial price. The pool is created with no
	/// liquidity, it must be added using the `PoolState::mint` function.
	///
	/// This function will panic if fee_pips or initial_sqrt_price are outside the allowed bounds
	pub fn new(fee_pips: u32, initial_sqrt_price: SqrtPriceQ64F96) -> Self {
		assert!(fee_pips <= ONE_IN_PIPS / 2);
		assert!(MIN_SQRT_PRICE <= initial_sqrt_price && initial_sqrt_price < MAX_SQRT_PRICE);
		let initial_tick = Self::tick_at_sqrt_price(initial_sqrt_price);
		Self {
			fee_pips,
			current_sqrt_price: initial_sqrt_price,
			current_tick: initial_tick,
			current_liquidity: 0,
			global_fee_growth: Default::default(),
			//  Guarantee MIN_TICK and MAX_TICK are always in map to simplify swap logic
			liquidity_map: [
				(
					MIN_TICK,
					TickInfo {
						liquidity_delta: 0,
						liquidity_gross: 0,
						fee_growth_outside: Default::default(),
					},
				),
				(
					MAX_TICK,
					TickInfo {
						liquidity_delta: 0,
						liquidity_gross: 0,
						fee_growth_outside: Default::default(),
					},
				),
			]
			.into(),
			positions: Default::default(),
		}
	}

	/// Tries to add `minted_liquidity` to/create the specified position, if `minted_liqudity == 0`
	/// no position will be created or have liquidity added, the callback will not be called, and
	/// the function will return `Ok(())`. Otherwise if the minting is possible the callback `f`
	/// will be passed the Amounts required to add the specified `minted_liquidity` to the specified
	/// position. The callback should return a boolean specifying if the liquidity minting should
	/// occur. If `false` is returned the position will not be created. If 'true' is returned the
	/// position will be created if it didn't already exist, and `minted_liquidity` will be added to
	/// it. Then the function will return `Ok(())`. Otherwise if the minting is not possible the
	/// callback will not be called, no state will be affected, and the function will return Err(_),
	/// with appropiate Error variant.
	///
	/// This function never panics
	///
	/// If this function returns an `Err(_)` no state changes have occurred
	pub fn mint<F: FnOnce(enum_map::EnumMap<Ticker, Amount>) -> bool>(
		&mut self,
		lp: LiquidityProvider,
		lower_tick: Tick,
		upper_tick: Tick,
		minted_liquidity: Liquidity,
		f: F,
	) -> Result<(), MintError> {
		if lower_tick < upper_tick && MIN_TICK <= lower_tick && upper_tick <= MAX_TICK {
			if minted_liquidity > 0 {
				let mut position =
					self.positions.get(&(lp, lower_tick, upper_tick)).cloned().unwrap_or_else(
						|| Position {
							liquidity: 0,
							last_fee_growth_inside: Default::default(),
							fees_owed: Default::default(),
						},
					);
				let tick_info_with_updated_gross_liquidity = |tick| {
					let mut tick_info =
						self.liquidity_map.get(&tick).cloned().unwrap_or_else(|| {
							TickInfo {
								liquidity_delta: 0,
								liquidity_gross: 0,
								fee_growth_outside: if tick <= self.current_tick {
									// by convention, we assume that all growth before a tick was
									// initialized happened _below_ the tick
									self.global_fee_growth
								} else {
									Default::default()
								},
							}
						});

					tick_info.liquidity_gross =
						u128::saturating_add(tick_info.liquidity_gross, minted_liquidity);
					if tick_info.liquidity_gross > MAX_TICK_GROSS_LIQUIDITY {
						Err(MintError::MaximumGrossLiquidity)
					} else {
						Ok(tick_info)
					}
				};

				let mut lower_info = tick_info_with_updated_gross_liquidity(lower_tick)?;
				// Cannot overflow as liquidity_delta.abs() is bounded to <=
				// MAX_TICK_GROSS_LIQUIDITY
				lower_info.liquidity_delta =
					lower_info.liquidity_delta.checked_add_unsigned(minted_liquidity).unwrap();
				let mut upper_info = tick_info_with_updated_gross_liquidity(upper_tick)?;
				// Cannot underflow as liquidity_delta.abs() is bounded to <=
				// MAX_TICK_GROSS_LIQUIDITY
				upper_info.liquidity_delta =
					upper_info.liquidity_delta.checked_sub_unsigned(minted_liquidity).unwrap();

				position.set_liquidity(
					&self,
					// Cannot overflow due to * MAX_TICK_GROSS_LIQUIDITY
					position.liquidity + minted_liquidity,
					lower_tick,
					&lower_info,
					upper_tick,
					&upper_info,
				);

				let (amounts_required, current_liquidity_delta) =
					self.liquidity_to_amounts::<true>(minted_liquidity, lower_tick, upper_tick);

				if f(amounts_required) {
					self.current_liquidity += current_liquidity_delta;
					self.positions.insert((lp, lower_tick, upper_tick), position);
					self.liquidity_map.insert(lower_tick, lower_info);
					self.liquidity_map.insert(upper_tick, upper_info);
				}
			}

			Ok(())
		} else {
			Err(MintError::InvalidTickRange)
		}
	}

	/// Tries to remove liquidity from the specified range-order, and convert the liqudity into
	/// Amounts owed to the LP. Also if the position no longer has any liquidity then it is
	/// destroyed and any fees earned by that position are also returned
	///
	/// This function never panics
	///
	/// If this function returns an `Err(_)` no state changes have occurred
	pub fn burn(
		&mut self,
		lp: LiquidityProvider,
		lower_tick: Tick,
		upper_tick: Tick,
		burnt_liquidity: Liquidity,
	) -> Result<
		(enum_map::EnumMap<Ticker, Amount>, enum_map::EnumMap<Ticker, u128>),
		PositionError<BurnError>,
	> {
		if let Some(mut position) = self.positions.get(&(lp, lower_tick, upper_tick)).cloned() {
			assert!(position.liquidity != 0);
			if burnt_liquidity <= position.liquidity {
				let mut lower_info = self.liquidity_map.get(&lower_tick).unwrap().clone();
				lower_info.liquidity_gross = lower_info.liquidity_gross - burnt_liquidity;
				lower_info.liquidity_delta =
					lower_info.liquidity_delta.checked_sub_unsigned(burnt_liquidity).unwrap();
				let mut upper_info = self.liquidity_map.get(&upper_tick).unwrap().clone();
				upper_info.liquidity_gross = lower_info.liquidity_gross - burnt_liquidity;
				upper_info.liquidity_delta =
					lower_info.liquidity_delta.checked_add_unsigned(burnt_liquidity).unwrap();

				position.set_liquidity(
					&self,
					position.liquidity - burnt_liquidity,
					lower_tick,
					&lower_info,
					upper_tick,
					&upper_info,
				);

				let (amounts_owed, current_liquidity_delta) =
					self.liquidity_to_amounts::<false>(burnt_liquidity, lower_tick, upper_tick);
				// Will not underflow as current_liquidity_delta must have previously been added to
				// current_liquidity for it to need to be substrated now
				self.current_liquidity -= current_liquidity_delta;

				let fees_owed = if position.liquidity == 0 {
					// DIFF: This behaviour is different than Uniswap's to ensure if a position
					// exists its ticks also exist in the liquidity_map
					self.positions.remove(&(lp, lower_tick, upper_tick));

					position.fees_owed
				} else {
					Default::default()
				};

				if lower_info.liquidity_gross == 0 && lower_tick != MIN_TICK
				// Guarantee MIN_TICK is always in map to simplify swap logic
				{
					assert_eq!(position.liquidity, 0);
					self.liquidity_map.remove(&lower_tick);
				} else {
					*self.liquidity_map.get_mut(&lower_tick).unwrap() = lower_info;
				}
				if upper_info.liquidity_gross == 0 && upper_tick != MAX_TICK
				// Guarantee MAX_TICK is always in map to simplify swap logic
				{
					assert_eq!(position.liquidity, 0);
					self.liquidity_map.remove(&upper_tick);
				} else {
					*self.liquidity_map.get_mut(&upper_tick).unwrap() = upper_info;
				}

				Ok((amounts_owed, fees_owed))
			} else {
				Err(PositionError::Other(BurnError::PositionLacksLiquidity))
			}
		} else {
			Err(PositionError::NonExistent)
		}
	}

	/// Tries to calculates the fees owed to the specified position, resets the fees owed for that
	/// position to zero, and returns the calculated amount of fees owed
	///
	/// This function never panics
	///
	/// If this function returns an `Err(_)` no state changes have occurred
	pub fn collect(
		&mut self,
		lp: LiquidityProvider,
		lower_tick: Tick,
		upper_tick: Tick,
	) -> Result<enum_map::EnumMap<Ticker, u128>, PositionError<CollectError>> {
		if let Some(mut position) = self.positions.get(&(lp, lower_tick, upper_tick)).cloned() {
			assert!(position.liquidity != 0);
			let lower_info = self.liquidity_map.get(&lower_tick).unwrap();
			let upper_info = self.liquidity_map.get(&upper_tick).unwrap();

			position.update_fees_owed(&self, lower_tick, &lower_info, upper_tick, &upper_info);

			let fees_owed = std::mem::take(&mut position.fees_owed);

			self.positions.insert((lp, lower_tick, upper_tick), position);

			Ok(fees_owed)
		} else {
			Err(PositionError::NonExistent)
		}
	}

	/// Swaps the specified Amount of Base into Pair, and returns the Pair Amount.
	///
	/// This function never panics
	pub fn swap_from_base_to_pair(&mut self, amount: Amount) -> Amount {
		self.swap::<BaseToPair>(amount)
	}

	/// Swaps the specified Amount of Pair into Base, and returns the Base Amount.
	///
	/// This function never panics
	pub fn swap_from_pair_to_base(&mut self, amount: Amount) -> Amount {
		self.swap::<PairToBase>(amount)
	}

	/// Swaps the specified Amount into the other currency, and returns the Amount. The direction of
	/// the swap is controlled by the generic type parameter `SD`, by setting it to `BaseToPair` or
	/// `PairToBase`.
	///
	/// This function never panics
	fn swap<SD: SwapDirection>(&mut self, mut amount: Amount) -> Amount {
		let mut total_amount_out = Amount::zero();

		while let Some((target_tick, target_info)) = (Amount::zero() != amount)
			.then_some(())
			.and_then(|()| SD::target_tick(self.current_tick, &mut self.liquidity_map))
		{
			let sqrt_ratio_target = Self::sqrt_price_at_tick(*target_tick);

			let amount_minus_fees = mul_div_floor(
				amount,
				U256::from(ONE_IN_PIPS - self.fee_pips),
				U256::from(ONE_IN_PIPS),
			); // This cannot overflow as we bound fee_pips to <= ONE_IN_PIPS/2 (TODO)

			let amount_required_to_reach_target = SD::input_amount_delta_ceil(
				self.current_sqrt_price,
				sqrt_ratio_target,
				self.current_liquidity,
			);

			let sqrt_ratio_next = if amount_minus_fees >= amount_required_to_reach_target {
				sqrt_ratio_target
			} else {
				assert!(self.current_liquidity != 0);
				SD::next_sqrt_price_from_input_amount(
					self.current_sqrt_price,
					self.current_liquidity,
					amount_minus_fees,
				)
			};

			// Cannot overflow as if the swap traversed all ticks (MIN_TICK to MAX_TICK
			// (inclusive)), assuming the maximum possible liquidity, total_amount_out would still
			// be below U256::MAX (See test `output_amounts_bounded`)
			total_amount_out += SD::output_amount_delta_floor(
				self.current_sqrt_price,
				sqrt_ratio_next,
				self.current_liquidity,
			);

			// next_sqrt_price_from_input_amount rounds so this maybe true even though
			// amount_minus_fees < amount_required_to_reach_target (TODO Prove)
			if sqrt_ratio_next == sqrt_ratio_target {
				// Note conversion to i128 and addition don't overflow (See test `max_liquidity`)
				self.current_liquidity = i128::try_from(self.current_liquidity)
					.unwrap()
					.checked_add(SD::liquidity_delta_on_crossing_tick(target_info))
					.unwrap()
					.try_into()
					.unwrap();

				// Will not overflow as fee_pips <= ONE_IN_PIPS / 2
				let fees = mul_div_ceil(
					amount_required_to_reach_target,
					U256::from(self.fee_pips),
					U256::from(ONE_IN_PIPS - self.fee_pips),
				);

				// DIFF: This behaviour is different to Uniswap's, we saturate instead of
				// overflowing/bricking the pool. This means we just stop giving LPs fees, but this
				// is exceptionally unlikely to occur due to the how large the maximum
				// global_fee_growth value is. We also do this to avoid needing to consider the
				// case of reverting an extrinsic's mutations which is expensive in Substrate based
				// chains.
				self.global_fee_growth[SD::INPUT_TICKER] =
					self.global_fee_growth[SD::INPUT_TICKER].saturating_add(fees);
				target_info.fee_growth_outside = enum_map::EnumMap::default().map(|ticker, ()| {
					self.global_fee_growth[ticker] - target_info.fee_growth_outside[ticker]
				});
				self.current_sqrt_price = sqrt_ratio_target;
				self.current_tick = SD::current_tick_after_crossing_target_tick(*target_tick);

				// TODO: Prove these don't underflow
				amount -= amount_required_to_reach_target;
				amount -= fees;
			} else {
				let amount_in = SD::input_amount_delta_ceil(
					self.current_sqrt_price,
					sqrt_ratio_next,
					self.current_liquidity,
				);
				// Will not underflow due to rounding in flavor of the pool of both sqrt_ratio_next
				// and amount_in. (TODO: Prove)
				let fees = amount - amount_in;

				// DIFF: This behaviour is different to Uniswap's,
				// we saturate instead of overflowing/bricking the pool. This means we just stop
				// giving LPs fees, but this is exceptionally unlikely to occur due to the how large
				// the maximum global_fee_growth value is. We also do this to avoid needing to
				// consider the case of reverting an extrinsic's mutations which is expensive in
				// Substrate based chains.
				self.global_fee_growth[SD::INPUT_TICKER] =
					self.global_fee_growth[SD::INPUT_TICKER].saturating_add(fees);
				self.current_sqrt_price = sqrt_ratio_next;
				self.current_tick = Self::tick_at_sqrt_price(self.current_sqrt_price);

				break
			};
		}

		total_amount_out
	}

	fn liquidity_to_amounts<const ROUND_UP: bool>(
		&self,
		liquidity: Liquidity,
		lower_tick: Tick,
		upper_tick: Tick,
	) -> (enum_map::EnumMap<Ticker, Amount>, Liquidity) {
		if self.current_tick < lower_tick {
			(
				enum_map::enum_map! {
					Ticker::Base => (if ROUND_UP { Self::base_amount_delta_ceil } else { Self::base_amount_delta_floor })(
						Self::sqrt_price_at_tick(lower_tick),
						Self::sqrt_price_at_tick(upper_tick),
						liquidity
					),
					Ticker::Pair => 0.into()
				},
				0,
			)
		} else if self.current_tick < upper_tick {
			(
				enum_map::enum_map! {
					Ticker::Base => (if ROUND_UP { Self::base_amount_delta_ceil } else { Self::base_amount_delta_floor })(
						self.current_sqrt_price,
						Self::sqrt_price_at_tick(upper_tick),
						liquidity
					),
					Ticker::Pair => (if ROUND_UP { Self::pair_amount_delta_ceil } else { Self::pair_amount_delta_floor })(
						Self::sqrt_price_at_tick(lower_tick),
						self.current_sqrt_price,
						liquidity
					)
				},
				liquidity,
			)
		} else {
			(
				enum_map::enum_map! {
					Ticker::Base => 0.into(),
					Ticker::Pair => (if ROUND_UP { Self::pair_amount_delta_ceil } else { Self::pair_amount_delta_floor })(
						Self::sqrt_price_at_tick(lower_tick),
						Self::sqrt_price_at_tick(upper_tick),
						liquidity
					)
				},
				0,
			)
		}
	}

	fn base_amount_delta_floor(
		from: SqrtPriceQ64F96,
		to: SqrtPriceQ64F96,
		liquidity: Liquidity,
	) -> Amount {
		assert!(SqrtPriceQ64F96::zero() < from);
		assert!(from <= to);

		/*
			Proof that `mul_div` does not overflow:
			If A ∈ ℕ, B ∈ ℕ, A > 0, B >= A
			Then A * B >= B and B - A < B
			Then A * B > B - A
		*/
		mul_div_floor(U256::from(liquidity) << 96u32, to - from, U256::full_mul(to, from))
	}

	fn base_amount_delta_ceil(
		from: SqrtPriceQ64F96,
		to: SqrtPriceQ64F96,
		liquidity: Liquidity,
	) -> Amount {
		assert!(SqrtPriceQ64F96::zero() < from);
		assert!(from <= to);

		/*
			Proof that `mul_div` does not overflow:
			If A ∈ ℕ, B ∈ ℕ, A > 0, B >= A
			Then A * B >= B and B - A < B
			Then A * B > B - A
		*/
		mul_div_ceil(U256::from(liquidity) << 96u32, to - from, U256::full_mul(to, from))
	}

	fn pair_amount_delta_floor(
		from: SqrtPriceQ64F96,
		to: SqrtPriceQ64F96,
		liquidity: Liquidity,
	) -> Amount {
		assert!(SqrtPriceQ64F96::zero() < from);
		assert!(from < to);

		/*
			Proof that `mul_div` does not overflow:
			If A ∈ u160, B ∈ u160, A < B, L ∈ u128
			Then B - A ∈ u160
			Then (B - A) / (1<<96) <= u64::MAX (160 - 96 = 64)
			Then L * ((B - A) / (1<<96)) <= u192::MAX < u256::MAX
		*/
		mul_div_floor(liquidity.into(), to - from, U512::from(1) << 96u32)
	}

	fn pair_amount_delta_ceil(
		from: SqrtPriceQ64F96,
		to: SqrtPriceQ64F96,
		liquidity: Liquidity,
	) -> Amount {
		assert!(SqrtPriceQ64F96::zero() < from);
		assert!(from < to);

		/*
			Proof that `mul_div` does not overflow:
			If A ∈ u160, B ∈ u160, A < B, L ∈ u128
			Then B - A ∈ u160
			Then (B - A) / (1<<96) <= u64::MAX (160 - 96 = 64)
			Then L * ((B - A) / (1<<96)) <= u192::MAX < u256::MAX
		*/
		mul_div_ceil(liquidity.into(), to - from, U512::from(1u32) << 96u32)
	}

	fn next_sqrt_price_from_base_input(
		sqrt_ratio_current: SqrtPriceQ64F96,
		liquidity: Liquidity,
		amount: Amount,
	) -> SqrtPriceQ64F96 {
		assert!(0 < liquidity);
		assert!(SqrtPriceQ64F96::zero() < sqrt_ratio_current);

		let liquidity = U256::from(liquidity) << 96u32;

		/*
			Proof that `mul_div` does not overflow:
			If L ∈ u256, R ∈ u256, A ∈ u256
			Then L <= L + R * A
			Then L / (L + R * A) <= 1
			Then R * L / (L + R * A) <= u256::MAX
		*/
		mul_div_ceil(
			liquidity,
			sqrt_ratio_current,
			// Addition will not overflow as function is not called if amount >=
			// amount_required_to_reach_target
			U512::from(liquidity) + U256::full_mul(amount, sqrt_ratio_current),
		)
	}

	fn next_sqrt_price_from_pair_input(
		sqrt_ratio_current: SqrtPriceQ64F96,
		liquidity: Liquidity,
		amount: Amount,
	) -> SqrtPriceQ64F96 {
		// Will not overflow as function is not called if amount >= amount_required_to_reach_target,
		// therefore bounding the function output to approximately <= MAX_SQRT_PRICE
		sqrt_ratio_current + amount / liquidity
	}

	fn sqrt_price_at_tick(tick: Tick) -> SqrtPriceQ64F96 {
		assert!(MIN_TICK <= tick && tick <= MAX_TICK);

		let abs_tick = tick.abs() as u32;

		let mut r = if abs_tick & 0x1u32 != 0 {
			U256::from(0xfffcb933bd6fad37aa2d162d1a594001u128)
		} else {
			U256::one() << 128u128
		};

		macro_rules! handle_tick_bit {
			($bit:literal, $constant:literal) => {
				/* Proof that `checked_mul` does not overflow:
					Note that the value ratio is initialized with above is such that `ratio <= (U256::one() << 128u128)`, alternatively `ratio <= (u128::MAX + 1)`
					First consider the case of applying the macro once assuming `ratio <= (U256::one() << 128u128)`:
						If ∀r ∈ U256, `r <= (U256::one() << 128u128)`, ∀C ∈ "Set of constants the macro is used with (Listed below)"
						Then `C * r <= U256::MAX` (See `debug_assertions` below)
						Therefore the `checked_mul` will not overflow
					Also note that above `(C * r >> 128u128) <= UINT128_MAX`
					Therefore if the if branch is taken ratio will be assigned a value `<= u128::MAX`
					else ratio is unchanged and remains `ratio <= u128::MAX + 1`
					Therefore as the assumption `ratio <= u128::MAX + 1` is always maintained after applying the macro,
					none of the checked_mul calls in any of the applications of the macro will overflow
				*/
				#[cfg(debug_assertions)]
				U256::checked_mul(U256::one() << 128u128, $constant.into()).unwrap();
				if abs_tick & (0x1u32 << $bit) != 0 {
					r = U256::checked_mul(r, U256::from($constant)).unwrap() >> 128u128
				}
			}
		}

		handle_tick_bit!(1, 0xfff97272373d413259a46990580e213au128);
		handle_tick_bit!(2, 0xfff2e50f5f656932ef12357cf3c7fdccu128);
		handle_tick_bit!(3, 0xffe5caca7e10e4e61c3624eaa0941cd0u128);
		handle_tick_bit!(4, 0xffcb9843d60f6159c9db58835c926644u128);
		handle_tick_bit!(5, 0xff973b41fa98c081472e6896dfb254c0u128);
		handle_tick_bit!(6, 0xff2ea16466c96a3843ec78b326b52861u128);
		handle_tick_bit!(7, 0xfe5dee046a99a2a811c461f1969c3053u128);
		handle_tick_bit!(8, 0xfcbe86c7900a88aedcffc83b479aa3a4u128);
		handle_tick_bit!(9, 0xf987a7253ac413176f2b074cf7815e54u128);
		handle_tick_bit!(10, 0xf3392b0822b70005940c7a398e4b70f3u128);
		handle_tick_bit!(11, 0xe7159475a2c29b7443b29c7fa6e889d9u128);
		handle_tick_bit!(12, 0xd097f3bdfd2022b8845ad8f792aa5825u128);
		handle_tick_bit!(13, 0xa9f746462d870fdf8a65dc1f90e061e5u128);
		handle_tick_bit!(14, 0x70d869a156d2a1b890bb3df62baf32f7u128);
		handle_tick_bit!(15, 0x31be135f97d08fd981231505542fcfa6u128);
		handle_tick_bit!(16, 0x9aa508b5b7a84e1c677de54f3e99bc9u128);
		handle_tick_bit!(17, 0x5d6af8dedb81196699c329225ee604u128);
		handle_tick_bit!(18, 0x2216e584f5fa1ea926041bedfe98u128);
		handle_tick_bit!(19, 0x48a170391f7dc42444e8fa2u128);
		// Note due to MIN_TICK and MAX_TICK bounds, past the 20th bit abs_tick is all zeros

		let sqrt_price_q32f128 = if tick > 0 { U256::MAX / r } else { r };

		// we round up in the division so tick_at_sqrt_price of the output price is always
		// consistent
		(sqrt_price_q32f128 >> 32u128) +
			if sqrt_price_q32f128.low_u32() == 0 { U256::zero() } else { U256::one() }
	}

	/// Calculates the greatest tick value such that `sqrt_price_at_tick(tick) <= sqrt_price`
	fn tick_at_sqrt_price(sqrt_price: SqrtPriceQ64F96) -> Tick {
		assert!(sqrt_price >= MIN_SQRT_PRICE);
		// Note the price can never actually reach MAX_SQRT_PRICE
		assert!(sqrt_price < MAX_SQRT_PRICE);

		let sqrt_price_q64f128 = U256::from(sqrt_price) << 32u128;

		let (integer_log_2, mantissa) = {
			let mut _bits_remaining = sqrt_price_q64f128;
			let mut most_signifcant_bit = 0u8;

			macro_rules! add_integer_bit {
				($bit:literal, $lower_bits_mask:literal) => {
					if _bits_remaining > U256::from($lower_bits_mask) {
						most_signifcant_bit = most_signifcant_bit | $bit;
						_bits_remaining = _bits_remaining >> $bit;
					}
				};
			}

			add_integer_bit!(128u8, 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFu128);
			add_integer_bit!(64u8, 0xFFFFFFFFFFFFFFFFu128);
			add_integer_bit!(32u8, 0xFFFFFFFFu128);
			add_integer_bit!(16u8, 0xFFFFu128);
			add_integer_bit!(8u8, 0xFFu128);
			add_integer_bit!(4u8, 0xFu128);
			add_integer_bit!(2u8, 0x3u128);
			add_integer_bit!(1u8, 0x1u128);

			(
				// most_signifcant_bit is the log2 of sqrt_price_q64f128 as an integer. This
				// converts most_signifcant_bit to the integer log2 of sqrt_price_q64f128 as an
				// q64f128
				((most_signifcant_bit as i16) + (-128i16)) as i8,
				// Calculate mantissa of sqrt_price_q64f128.
				if most_signifcant_bit >= 128u8 {
					// The bits we possibly drop when right shifting don't contribute to the log2
					// above the 14th fractional bit.
					sqrt_price_q64f128 >> (most_signifcant_bit - 127u8)
				} else {
					sqrt_price_q64f128 << (127u8 - most_signifcant_bit)
				}
				.as_u128(), // Conversion to u128 is safe as top 128 bits are always zero
			)
		};

		let log_2_q63f64 = {
			let mut log_2_q63f64 = (integer_log_2 as i128) << 64u8;
			let mut _mantissa = mantissa;

			macro_rules! add_fractional_bit {
				($bit:literal) => {
					// Note squaring a number doubles its log
					let mantissa_sq =
						(U256::checked_mul(_mantissa.into(), _mantissa.into()).unwrap() >> 127u8);
					_mantissa = if mantissa_sq.bit(128) {
						// is the 129th bit set, all higher bits must be zero due to 127 right bit
						// shift
						log_2_q63f64 = log_2_q63f64 | (1i128 << $bit);
						(mantissa_sq >> 1u8).as_u128()
					} else {
						mantissa_sq.as_u128()
					}
				};
			}

			add_fractional_bit!(63u8);
			add_fractional_bit!(62u8);
			add_fractional_bit!(61u8);
			add_fractional_bit!(60u8);
			add_fractional_bit!(59u8);
			add_fractional_bit!(58u8);
			add_fractional_bit!(57u8);
			add_fractional_bit!(56u8);
			add_fractional_bit!(55u8);
			add_fractional_bit!(54u8);
			add_fractional_bit!(53u8);
			add_fractional_bit!(52u8);
			add_fractional_bit!(51u8);
			add_fractional_bit!(50u8);

			// We don't need more precision than (63..50) = 14 bits

			log_2_q63f64
		};

		// Note we don't have a I256 type so I have to handle the negative mul case manually
		let log_sqrt10001_q127f128 = U256::overflowing_mul(
			if log_2_q63f64 < 0 {
				(U256::from(u128::MAX) << 128u8) | U256::from(log_2_q63f64 as u128)
			} else {
				U256::from(log_2_q63f64 as u128)
			},
			U256::from(255738958999603826347141u128),
		)
		.0;

		let tick_low = (U256::overflowing_sub(
			log_sqrt10001_q127f128,
			U256::from(3402992956809132418596140100660247210u128),
		)
		.0 >> 128u8)
			.as_u128() as Tick; // Add Checks
		let tick_high = (U256::overflowing_add(
			log_sqrt10001_q127f128,
			U256::from(291339464771989622907027621153398088495u128),
		)
		.0 >> 128u8)
			.as_u128() as Tick; // Add Checks

		if tick_low == tick_high {
			tick_low
		} else {
			if Self::sqrt_price_at_tick(tick_high) <= sqrt_price {
				tick_high
			} else {
				tick_low
			}
		}
	}
}

#[cfg(test)]
mod test {
	use super::*;

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
	fn output_amounts_bounded() {
		// Note these values are significant over-estimates of the maximum output amount
		PairToBase::output_amount_delta_floor(
			PoolState::sqrt_price_at_tick(MIN_TICK),
			PoolState::sqrt_price_at_tick(MAX_TICK),
			MAX_TICK_GROSS_LIQUIDITY,
		)
		.checked_mul((1 + MAX_TICK - MIN_TICK).into())
		.unwrap();
		BaseToPair::output_amount_delta_floor(
			PoolState::sqrt_price_at_tick(MAX_TICK),
			PoolState::sqrt_price_at_tick(MIN_TICK),
			MAX_TICK_GROSS_LIQUIDITY,
		)
		.checked_mul((1 + MAX_TICK - MIN_TICK).into())
		.unwrap();
	}

	#[test]
	fn test_sqrt_price_at_tick() {
		assert_eq!(PoolState::sqrt_price_at_tick(MIN_TICK), MIN_SQRT_PRICE);
		assert_eq!(
			PoolState::sqrt_price_at_tick(-738203),
			U256::from_dec_str("7409801140451").unwrap()
		);
		assert_eq!(
			PoolState::sqrt_price_at_tick(-500000),
			U256::from_dec_str("1101692437043807371").unwrap()
		);
		assert_eq!(
			PoolState::sqrt_price_at_tick(-250000),
			U256::from_dec_str("295440463448801648376846").unwrap()
		);
		assert_eq!(
			PoolState::sqrt_price_at_tick(-150000),
			U256::from_dec_str("43836292794701720435367485").unwrap()
		);
		assert_eq!(
			PoolState::sqrt_price_at_tick(-50000),
			U256::from_dec_str("6504256538020985011912221507").unwrap()
		);
		assert_eq!(
			PoolState::sqrt_price_at_tick(-5000),
			U256::from_dec_str("61703726247759831737814779831").unwrap()
		);
		assert_eq!(
			PoolState::sqrt_price_at_tick(-4000),
			U256::from_dec_str("64867181785621769311890333195").unwrap()
		);
		assert_eq!(
			PoolState::sqrt_price_at_tick(-3000),
			U256::from_dec_str("68192822843687888778582228483").unwrap()
		);
		assert_eq!(
			PoolState::sqrt_price_at_tick(-2500),
			U256::from_dec_str("69919044979842180277688105136").unwrap()
		);
		assert_eq!(
			PoolState::sqrt_price_at_tick(-1000),
			U256::from_dec_str("75364347830767020784054125655").unwrap()
		);
		assert_eq!(
			PoolState::sqrt_price_at_tick(-500),
			U256::from_dec_str("77272108795590369356373805297").unwrap()
		);
		assert_eq!(
			PoolState::sqrt_price_at_tick(-250),
			U256::from_dec_str("78244023372248365697264290337").unwrap()
		);
		assert_eq!(
			PoolState::sqrt_price_at_tick(-100),
			U256::from_dec_str("78833030112140176575862854579").unwrap()
		);
		assert_eq!(
			PoolState::sqrt_price_at_tick(-50),
			U256::from_dec_str("79030349367926598376800521322").unwrap()
		);
		assert_eq!(
			PoolState::sqrt_price_at_tick(50),
			U256::from_dec_str("79426470787362580746886972461").unwrap()
		);
		assert_eq!(
			PoolState::sqrt_price_at_tick(100),
			U256::from_dec_str("79625275426524748796330556128").unwrap()
		);
		assert_eq!(
			PoolState::sqrt_price_at_tick(250),
			U256::from_dec_str("80224679980005306637834519095").unwrap()
		);
		assert_eq!(
			PoolState::sqrt_price_at_tick(500),
			U256::from_dec_str("81233731461783161732293370115").unwrap()
		);
		assert_eq!(
			PoolState::sqrt_price_at_tick(1000),
			U256::from_dec_str("83290069058676223003182343270").unwrap()
		);
		assert_eq!(
			PoolState::sqrt_price_at_tick(2500),
			U256::from_dec_str("89776708723587163891445672585").unwrap()
		);
		assert_eq!(
			PoolState::sqrt_price_at_tick(3000),
			U256::from_dec_str("92049301871182272007977902845").unwrap()
		);
		assert_eq!(
			PoolState::sqrt_price_at_tick(4000),
			U256::from_dec_str("96768528593268422080558758223").unwrap()
		);
		assert_eq!(
			PoolState::sqrt_price_at_tick(5000),
			U256::from_dec_str("101729702841318637793976746270").unwrap()
		);
		assert_eq!(
			PoolState::sqrt_price_at_tick(50000),
			U256::from_dec_str("965075977353221155028623082916").unwrap()
		);
		assert_eq!(
			PoolState::sqrt_price_at_tick(150000),
			U256::from_dec_str("143194173941309278083010301478497").unwrap()
		);
		assert_eq!(
			PoolState::sqrt_price_at_tick(250000),
			U256::from_dec_str("21246587762933397357449903968194344").unwrap()
		);
		assert_eq!(
			PoolState::sqrt_price_at_tick(500000),
			U256::from_dec_str("5697689776495288729098254600827762987878").unwrap()
		);
		assert_eq!(
			PoolState::sqrt_price_at_tick(738203),
			U256::from_dec_str("847134979253254120489401328389043031315994541").unwrap()
		);
		assert_eq!(PoolState::sqrt_price_at_tick(MAX_TICK), MAX_SQRT_PRICE);
	}

	#[test]
	fn test_tick_at_sqrt_price() {
		assert_eq!(PoolState::tick_at_sqrt_price(MIN_SQRT_PRICE), MIN_TICK);
		assert_eq!(
			PoolState::tick_at_sqrt_price(U256::from_dec_str("79228162514264337593543").unwrap()),
			-276325
		);
		assert_eq!(
			PoolState::tick_at_sqrt_price(
				U256::from_dec_str("79228162514264337593543950").unwrap()
			),
			-138163
		);
		assert_eq!(
			PoolState::tick_at_sqrt_price(
				U256::from_dec_str("9903520314283042199192993792").unwrap()
			),
			-41591
		);
		assert_eq!(
			PoolState::tick_at_sqrt_price(
				U256::from_dec_str("28011385487393069959365969113").unwrap()
			),
			-20796
		);
		assert_eq!(
			PoolState::tick_at_sqrt_price(
				U256::from_dec_str("56022770974786139918731938227").unwrap()
			),
			-6932
		);
		assert_eq!(
			PoolState::tick_at_sqrt_price(
				U256::from_dec_str("79228162514264337593543950336").unwrap()
			),
			0
		);
		assert_eq!(
			PoolState::tick_at_sqrt_price(
				U256::from_dec_str("112045541949572279837463876454").unwrap()
			),
			6931
		);
		assert_eq!(
			PoolState::tick_at_sqrt_price(
				U256::from_dec_str("224091083899144559674927752909").unwrap()
			),
			20795
		);
		assert_eq!(
			PoolState::tick_at_sqrt_price(
				U256::from_dec_str("633825300114114700748351602688").unwrap()
			),
			41590
		);
		assert_eq!(
			PoolState::tick_at_sqrt_price(
				U256::from_dec_str("79228162514264337593543950336000").unwrap()
			),
			138162
		);
		assert_eq!(
			PoolState::tick_at_sqrt_price(
				U256::from_dec_str("79228162514264337593543950336000000").unwrap()
			),
			276324
		);
		assert_eq!(PoolState::tick_at_sqrt_price(MAX_SQRT_PRICE - 1), MAX_TICK - 1);
	}
}
