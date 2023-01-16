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

#![feature(mixed_integer_ops)] // This has been stablized in a future version of rust

use std::{collections::BTreeMap, default, u128};

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

#[derive(Clone, Debug)]
struct Position {
	liquidity: Liquidity,
	last_fee_growth_inside: enum_map::EnumMap<Ticker, FeeGrowthQ128F128>,
}
impl Position {
	fn update_fees_owed(
		&mut self,
		pool_state: &PoolState,
		lower_tick: Tick,
		lower_info: &TickInfo,
		upper_tick: Tick,
		upper_info: &TickInfo,
	) -> enum_map::EnumMap<Ticker, u128> {
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
		let fees_owed = enum_map::EnumMap::default().map(|ticker, ()| {
			// DIFF: This behaviour is different than Uniswap's. We saturate fees_owed instead of
			// overflowing

			/*
				Proof that `mul_div` does not overflow:
				Note position.liqiudity: u128
				U512::one() << 128 > u128::MAX
			*/
			mul_div_floor(
				fee_growth_inside[ticker] - self.last_fee_growth_inside[ticker],
				self.liquidity.into(),
				U512::one() << 128,
			)
			.try_into()
			.unwrap_or(u128::MAX)
		});
		self.last_fee_growth_inside = fee_growth_inside;
		fees_owed
	}

	fn set_liquidity(
		&mut self,
		pool_state: &PoolState,
		new_liquidity: Liquidity,
		lower_tick: Tick,
		lower_info: &TickInfo,
		upper_tick: Tick,
		upper_info: &TickInfo,
	) -> enum_map::EnumMap<Ticker, u128> {
		let fees_owed =
			self.update_fees_owed(pool_state, lower_tick, lower_info, upper_tick, upper_info);
		self.liquidity = new_liquidity;
		fees_owed
	}
}

#[derive(Clone, Debug)]
struct TickInfo {
	liquidity_delta: i128,
	liquidity_gross: u128,
	fee_growth_outside: enum_map::EnumMap<Ticker, FeeGrowthQ128F128>,
}

#[derive(Debug)]
pub struct PoolState {
	fee_pips: u32,
	current_sqrt_price: SqrtPriceQ64F96,
	current_tick: Tick,
	current_liquidity: Liquidity,
	global_fee_growth: enum_map::EnumMap<Ticker, FeeGrowthQ128F128>,
	liquidity_map: BTreeMap<Tick, TickInfo>,
	positions: BTreeMap<(LiquidityProvider, Tick, Tick), Position>,
}

#[derive(Enum, Clone, Copy, Debug)]
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

#[derive(Debug)]
pub enum MintError {
	/// Invalid Tick range
	InvalidTickRange,
	/// One of the start/end ticks of the range reached its maximm gross liquidity
	MaximumGrossLiquidity,
}

#[derive(Debug)]
pub enum PositionError<T> {
	/// Position referenced does not exist
	NonExistent,
	Other(T),
}

#[derive(Debug)]
pub enum BurnError {
	/// Position referenced does not contain the requested liquidity
	PositionLacksLiquidity,
}

#[derive(Debug)]
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
	) -> Result<enum_map::EnumMap<Ticker, u128>, MintError> {
		if lower_tick < upper_tick && MIN_TICK <= lower_tick && upper_tick <= MAX_TICK {
			let mut fees_owed = Default::default();
			if minted_liquidity > 0 {
				let mut position =
					self.positions.get(&(lp, lower_tick, upper_tick)).cloned().unwrap_or_else(
						|| Position { liquidity: 0, last_fee_growth_inside: Default::default() },
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

				fees_owed = position.set_liquidity(
					self,
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

			Ok(fees_owed)
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
	#[allow(clippy::type_complexity)]
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
				lower_info.liquidity_gross -= burnt_liquidity;
				lower_info.liquidity_delta =
					lower_info.liquidity_delta.checked_sub_unsigned(burnt_liquidity).unwrap();
				let mut upper_info = self.liquidity_map.get(&upper_tick).unwrap().clone();
				upper_info.liquidity_gross -= burnt_liquidity;
				upper_info.liquidity_delta =
					lower_info.liquidity_delta.checked_add_unsigned(burnt_liquidity).unwrap();

				let fees_owed = position.set_liquidity(
					self,
					position.liquidity - burnt_liquidity,
					lower_tick,
					&lower_info,
					upper_tick,
					&upper_info,
				);
				// DIFF: This behaviour is different than Uniswap's. Burnt liquidity (amounts_owed)
				// is not stored as tokensOwed in the position but it's only returned as a result of
				// this function.
				let (amounts_owed, current_liquidity_delta) =
					self.liquidity_to_amounts::<false>(burnt_liquidity, lower_tick, upper_tick);
				// Will not underflow as current_liquidity_delta must have previously been added to
				// current_liquidity for it to need to be substrated now
				self.current_liquidity -= current_liquidity_delta;

				if position.liquidity == 0 {
					// DIFF: This behaviour is different than Uniswap's to ensure if a position
					// exists its ticks also exist in the liquidity_map
					// In other words, the position will automatically be removed if all the
					// liquidity has been burnt.
					self.positions.remove(&(lp, lower_tick, upper_tick));
				}

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

				// BUGFIX: We must update the position's liquidity, otherwise the LP would
				// effectively be able to burn liquidity from other position. This will also update
				// the fees owed, which is not strictly necessary but it's a good idea (how Uniswap
				// works). Could also be added as part of the if/else before but then we need to
				// clone position to get ownership.
				if position.liquidity != 0 {
					self.positions.insert((lp, lower_tick, upper_tick), position);
				}

				Ok((amounts_owed, fees_owed))
			} else {
				Err(PositionError::Other(BurnError::PositionLacksLiquidity))
			}
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
				// BUGFIX: fees need to be divided by the current_liquidity to get the fee growth
				// BUGFIX2: Update only if currentliquditiy > 0 to avoid division by zero
				if self.current_liquidity > 0 {
					self.global_fee_growth[SD::INPUT_TICKER] =
						self.global_fee_growth[SD::INPUT_TICKER].saturating_add(mul_div_floor(
							fees,
							U256::from(1) << 128u32,
							self.current_liquidity,
						));
					target_info.fee_growth_outside =
						enum_map::EnumMap::default().map(|ticker, ()| {
							self.global_fee_growth[ticker] - target_info.fee_growth_outside[ticker]
						});
				}

				self.current_sqrt_price = sqrt_ratio_target;
				self.current_tick = SD::current_tick_after_crossing_target_tick(*target_tick);

				// TODO: Prove these don't underflow
				amount -= amount_required_to_reach_target;
				amount -= fees;

				// Since the liquidity value is used for the fee calculation, updating needs to be
				// done at the end.
				// Note conversion to i128 and addition don't overflow (See test `max_liquidity`)
				self.current_liquidity = i128::try_from(self.current_liquidity)
					.unwrap()
					.checked_add(SD::liquidity_delta_on_crossing_tick(target_info))
					.unwrap()
					.try_into()
					.unwrap();
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
				// BUGFIX: fees need to be divided by the current_liquidity to get the fee growth
				// BUGFIX2: Update only if currentliquditiy > 0 to avoid division by zero
				if self.current_liquidity > 0 {
					self.global_fee_growth[SD::INPUT_TICKER] =
						self.global_fee_growth[SD::INPUT_TICKER].saturating_add(mul_div_floor(
							fees,
							U256::from(1) << 128u32,
							self.current_liquidity,
						));
				}
				// BUGFIXING: recompute unless we're on a lower tick boundary (i.e. already
				// transitioned ticks), and haven't moved (test_updates_exiting &
				// test_updates_entering)
				if self.current_sqrt_price != sqrt_ratio_next {
					self.current_sqrt_price = sqrt_ratio_next;
					self.current_tick = Self::tick_at_sqrt_price(self.current_sqrt_price);
				}

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
		// BUGFIX: When minting/burning at lowertick == currenttick, from == to. When swapping only
		// from < to. To refine the check?
		assert!(from <= to);

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
		// BUGFIX: When minting/burning at lowertick == currenttick, from == to. When swapping only
		// from < to. To refine the check?
		assert!(from <= to);

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
		sqrt_ratio_current + (amount << 96u32) / liquidity
	}

	fn sqrt_price_at_tick(tick: Tick) -> SqrtPriceQ64F96 {
		assert!((MIN_TICK..=MAX_TICK).contains(&tick));

		let abs_tick = tick.unsigned_abs();

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

		let sqrt_price_q64f128 = sqrt_price << 32u128;

		let (integer_log_2, mantissa) = {
			let mut bits_remaining = sqrt_price_q64f128;
			let mut most_signifcant_bit = 0u8;

			// rustfmt chokes when formatting this macro.
			// See: https://github.com/rust-lang/rustfmt/issues/5404
			#[rustfmt::skip]
			macro_rules! add_integer_bit {
				($bit:literal, $lower_bits_mask:literal) => {
					if bits_remaining > U256::from($lower_bits_mask) {
						most_signifcant_bit |= $bit;
						bits_remaining >>= $bit;
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

			// rustfmt chokes when formatting this macro.
			// See: https://github.com/rust-lang/rustfmt/issues/5404
			#[rustfmt::skip]
			macro_rules! add_fractional_bit {
				($bit:literal) => {
					// Note squaring a number doubles its log
					let mantissa_sq =
						(U256::checked_mul(_mantissa.into(), _mantissa.into()).unwrap() >> 127u8);
					_mantissa = if mantissa_sq.bit(128) {
						// is the 129th bit set, all higher bits must be zero due to 127 right bit
						// shift
						log_2_q63f64 |= (1i128 << $bit);
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
		} else if Self::sqrt_price_at_tick(tick_high) <= sqrt_price {
			tick_high
		} else {
			tick_low
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

	// UNISWAP TESTS => UniswapV3Pool.spec.ts

	pub const TICKSPACING_UNISWAP_MEDIUM: Tick = 60;
	pub const MIN_TICK_UNISWAP_MEDIUM: Tick = -887220;
	pub const MAX_TICK_UNISWAP_MEDIUM: Tick = -MIN_TICK_UNISWAP_MEDIUM;

	fn mint_pool() -> (PoolState, enum_map::EnumMap<Ticker, Amount>, LiquidityProvider) {
		let mut pool =
			PoolState::new(300, U256::from_dec_str("25054144837504793118650146401").unwrap()); // encodeSqrtPrice (1,10)
		const ID: LiquidityProvider = H256([0xcf; 32]);
		const MINTED_LIQUIDITY: u128 = 3_161;
		let mut minted_capital = None;

		let fees_owed = pool
			.mint(
				ID,
				MIN_TICK_UNISWAP_MEDIUM,
				MAX_TICK_UNISWAP_MEDIUM,
				MINTED_LIQUIDITY,
				|minted| {
					minted_capital.replace(minted);
					true
				},
			)
			.unwrap();
		let minted_capital = minted_capital.unwrap();

		(pool, minted_capital, ID)
	}

	#[test]
	#[should_panic]
	fn test_initialize_failure() {
		PoolState::new(100, U256::from(1));
	}
	#[test]
	fn test_initialize_success() {
		PoolState::new(100, U256::from(MIN_SQRT_PRICE));
		PoolState::new(100, U256::from(MAX_SQRT_PRICE - 1));

		let pool =
			PoolState::new(100, U256::from_dec_str("56022770974786143748341366784").unwrap());

		assert_eq!(
			pool.current_sqrt_price,
			U256::from_dec_str("56022770974786143748341366784").unwrap()
		);
		assert_eq!(pool.current_tick, Tick::from(-6_932));
	}
	#[test]
	#[should_panic]
	fn test_initialize_too_low() {
		PoolState::new(100, U256::from(MIN_SQRT_PRICE - 1));
	}

	#[test]
	#[should_panic]
	fn test_initialize_too_high() {
		PoolState::new(100, U256::from(MAX_SQRT_PRICE));
	}

	#[test]
	#[should_panic]
	fn test_initialize_too_high_2() {
		PoolState::new(
			100,
			U256::from_dec_str(
				"57896044618658097711785492504343953926634992332820282019728792003956564819968", /* 2**160-1 */
			)
			.unwrap(),
		);
	}

	// Minting

	// Failure - TBD: Check that the minting failures don't modify the pool

	#[test]
	fn test_mint_err() {
		let (mut pool, _, id) = mint_pool();
		assert!(pool.mint(id, 1, 0, 1, |_| true).is_err());
		assert!((pool.mint(id, -887273, 0, 1, |_| true)).is_err());
		assert!((pool.mint(id, 0, 887273, 1, |_| true)).is_err());

		assert!((pool.mint(
			id,
			MIN_TICK_UNISWAP_MEDIUM + 1,
			MAX_TICK_UNISWAP_MEDIUM - 1,
			MAX_TICK_GROSS_LIQUIDITY + 1,
			|_| true
		))
		.is_err());

		assert!((pool.mint(
			id,
			MIN_TICK_UNISWAP_MEDIUM + 1,
			MAX_TICK_UNISWAP_MEDIUM - 1,
			MAX_TICK_GROSS_LIQUIDITY,
			|_| true
		))
		.is_ok());
	}

	#[test]
	fn test_mint_err_tickmax() {
		let (mut pool, _, id) = mint_pool();

		let fees_owed = pool
			.mint(id, MIN_TICK_UNISWAP_MEDIUM + 1, MAX_TICK_UNISWAP_MEDIUM - 1, 1000, |_| true)
			.unwrap();

		//assert_eq!(fees_owed.unwrap()[Ticker::Base], 0);
		// assert_eq!(fees_owed.unwrap()[Ticker::Pair], 0);
		match (fees_owed[Ticker::Base], fees_owed[Ticker::Pair]) {
			(0, 0) => {},
			_ => panic!("Fees accrued are not zero"),
		}

		assert!((pool.mint(
			id,
			MIN_TICK_UNISWAP_MEDIUM + 1,
			MAX_TICK_UNISWAP_MEDIUM - 1,
			MAX_TICK_GROSS_LIQUIDITY - 1000 + 1,
			|_| true
		))
		.is_err());

		assert!((pool.mint(
			id,
			MIN_TICK_UNISWAP_MEDIUM + 2,
			MAX_TICK_UNISWAP_MEDIUM - 1,
			MAX_TICK_GROSS_LIQUIDITY - 1000 + 1,
			|_| true
		))
		.is_err());

		assert!((pool.mint(
			id,
			MIN_TICK_UNISWAP_MEDIUM + 1,
			MAX_TICK_UNISWAP_MEDIUM - 2,
			MAX_TICK_GROSS_LIQUIDITY - 1000 + 1,
			|_| true
		))
		.is_err());

		let fees_owed = pool
			.mint(
				id,
				MIN_TICK_UNISWAP_MEDIUM + 1,
				MAX_TICK_UNISWAP_MEDIUM - 1,
				MAX_TICK_GROSS_LIQUIDITY - 1000,
				|_| true,
			)
			.unwrap();
		match (fees_owed[Ticker::Base], fees_owed[Ticker::Pair]) {
			(0, 0) => {},
			_ => panic!("Fees accrued are not zero"),
		}

		// Different behaviour from Uniswap - does not revert when minting 0
		let fees_owed = pool
			.mint(id, MIN_TICK_UNISWAP_MEDIUM + 1, MAX_TICK_UNISWAP_MEDIUM - 1, 0, |_| true)
			.unwrap();
		match (fees_owed[Ticker::Base], fees_owed[Ticker::Pair]) {
			(0, 0) => {},
			_ => panic!("Fees accrued are not zero"),
		}
	}

	// Success cases

	#[test]
	fn test_balances() {
		let (_, minted_capital, _) = mint_pool();
		// Check "balances"
		const INPUT_TICKER: Ticker = Ticker::Base;
		assert_eq!(minted_capital[INPUT_TICKER], U256::from(9_996));
		assert_eq!(minted_capital[!INPUT_TICKER], U256::from(1_000));
	}

	#[test]
	fn test_initial_tick() {
		let (pool, _, _) = mint_pool();
		// Check current tick
		assert_eq!(pool.current_tick, Tick::from(-23_028));
	}

	#[test]
	fn above_current_price() {
		let (mut pool, mut minted_capital_accum, id) = mint_pool();

		const MINTED_LIQUIDITY: u128 = 10_000;
		const INPUT_TICKER: Ticker = Ticker::Base;

		let mut minted_capital = None;
		let fees_owed = pool
			.mint(id, -22980, 0, MINTED_LIQUIDITY, |minted| {
				minted_capital.replace(minted);
				true
			})
			.unwrap();
		let minted_capital = minted_capital.unwrap();

		match (fees_owed[Ticker::Base], fees_owed[Ticker::Pair]) {
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
				true
			},
		)
		.unwrap();
		let minted_capital = minted_capital.unwrap();

		minted_capital_accum[Ticker::Base] += minted_capital[Ticker::Base];
		minted_capital_accum[!Ticker::Base] += minted_capital[!Ticker::Base];

		assert_eq!(minted_capital_accum[Ticker::Base], U256::from(9_996 + 828_011_525));
		assert_eq!(minted_capital_accum[!Ticker::Base], U256::from(1_000));
	}

	#[test]
	fn test_maxtick() {
		let (mut pool, mut minted_capital_accum, id) = mint_pool();
		let mut minted_capital = None;
		pool.mint(id, -22980, 887220, 10000, |minted| {
			minted_capital.replace(minted);
			true
		})
		.unwrap();
		let minted_capital = minted_capital.unwrap();

		minted_capital_accum[Ticker::Base] += minted_capital[Ticker::Base];
		minted_capital_accum[!Ticker::Base] += minted_capital[!Ticker::Base];

		assert_eq!(minted_capital_accum[Ticker::Base], U256::from(9_996 + 31_549));
		assert_eq!(minted_capital_accum[!Ticker::Base], U256::from(1_000));
	}

	#[test]
	fn test_removing_works_0() {
		let (mut pool, _, id) = mint_pool();
		let mut minted_capital = None;
		pool.mint(id, -240, 0, 10000, |minted| {
			minted_capital.replace(minted);
			true
		})
		.unwrap();

		let (returned_capital, fees_owed) = pool.burn(id, -240, 0, 10000).unwrap();

		assert_eq!(returned_capital[Ticker::Base], U256::from(120));
		assert_eq!(returned_capital[!Ticker::Base], U256::from(0));

		assert_eq!(fees_owed[Ticker::Base], 0);
		assert_eq!(fees_owed[!Ticker::Base], 0);
	}

	#[test]
	fn test_removing_works_twosteps_0() {
		let (mut pool, _, id) = mint_pool();
		let mut minted_capital = None;
		pool.mint(id, -240, 0, 10000, |minted| {
			minted_capital.replace(minted);
			true
		})
		.unwrap();

		let (returned_capital_0, fees_owed_0) = pool.burn(id, -240, 0, 10000 / 2).unwrap();
		let (returned_capital_1, fees_owed_1) = pool.burn(id, -240, 0, 10000 / 2).unwrap();

		assert_eq!(returned_capital_0[Ticker::Base], U256::from(60));
		assert_eq!(returned_capital_0[!Ticker::Base], U256::from(0));
		assert_eq!(returned_capital_1[Ticker::Base], U256::from(60));
		assert_eq!(returned_capital_1[!Ticker::Base], U256::from(0));

		assert_eq!(fees_owed_0[Ticker::Base], 0);
		assert_eq!(fees_owed_0[!Ticker::Base], 0);
		assert_eq!(fees_owed_1[Ticker::Base], 0);
		assert_eq!(fees_owed_1[!Ticker::Base], 0);
	}

	#[test]
	fn test_addliquidityto_liquiditygross() {
		let (mut pool, _, id) = mint_pool();
		let fees_owed = pool.mint(id, -240, 0, 100, |_| true).unwrap();

		match (fees_owed[Ticker::Base], fees_owed[Ticker::Pair]) {
			(0, 0) => {},
			_ => panic!("Fees accrued are not zero"),
		}

		assert_eq!(pool.liquidity_map.get(&-240).unwrap().liquidity_gross, 100);
		assert_eq!(pool.liquidity_map.get(&0).unwrap().liquidity_gross, 100);
		assert!(!pool.liquidity_map.contains_key(&1));
		assert!(!pool.liquidity_map.contains_key(&2));

		let fees_owed = pool.mint(id, -240, 1, 150, |_| true).unwrap();

		match (fees_owed[Ticker::Base], fees_owed[Ticker::Pair]) {
			(0, 0) => {},
			_ => panic!("Fees accrued are not zero"),
		}
		assert_eq!(pool.liquidity_map.get(&-240).unwrap().liquidity_gross, 250);
		assert_eq!(pool.liquidity_map.get(&0).unwrap().liquidity_gross, 100);
		assert_eq!(pool.liquidity_map.get(&1).unwrap().liquidity_gross, 150);
		assert!(!pool.liquidity_map.contains_key(&2));

		let fees_owed = pool.mint(id, 0, 2, 60, |_| true).unwrap();

		match (fees_owed[Ticker::Base], fees_owed[Ticker::Pair]) {
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
		pool.mint(id, -240, 0, 100, |_| true).unwrap();
		pool.mint(id, -240, 0, 40, |_| true).unwrap();
		let (_, fees_owed) = pool.burn(id, -240, 0, 90).unwrap();
		match (fees_owed[Ticker::Base], fees_owed[Ticker::Pair]) {
			(0, 0) => {},
			_ => panic!("Fees accrued are not zero"),
		}
		assert_eq!(pool.liquidity_map.get(&-240).unwrap().liquidity_gross, 50);
		assert_eq!(pool.liquidity_map.get(&0).unwrap().liquidity_gross, 50);
	}

	#[test]
	fn test_clearsticklower_ifpositionremoved() {
		let (mut pool, _, id) = mint_pool();
		pool.mint(id, -240, 0, 100, |_| true).unwrap();
		let (_, fees_owed) = pool.burn(id, -240, 0, 100).unwrap();
		match (fees_owed[Ticker::Base], fees_owed[Ticker::Pair]) {
			(0, 0) => {},
			_ => panic!("Fees accrued are not zero"),
		}
		assert!(!pool.liquidity_map.contains_key(&-240));
	}

	#[test]
	fn test_clearstickupper_ifpositionremoved() {
		let (mut pool, _, id) = mint_pool();
		pool.mint(id, -240, 0, 100, |_| true).unwrap();
		pool.burn(id, -240, 0, 100).unwrap();
		assert!(!pool.liquidity_map.contains_key(&0));
	}

	#[test]
	fn test_clears_onlyunused() {
		let (mut pool, _, id) = mint_pool();
		pool.mint(id, -240, 0, 100, |_| true).unwrap();
		pool.mint(id, -60, 0, 250, |_| true).unwrap();
		pool.burn(id, -240, 0, 100).unwrap();
		assert!(!pool.liquidity_map.contains_key(&-240));
		assert_eq!(pool.liquidity_map.get(&0).unwrap().liquidity_gross, 250);
		assert_eq!(
			pool.liquidity_map.get(&0).unwrap().fee_growth_outside[Ticker::Base],
			U256::from(0)
		);
		assert_eq!(
			pool.liquidity_map.get(&0).unwrap().fee_growth_outside[!Ticker::Base],
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
				true
			},
		)
		.unwrap();
		let minted_capital = minted_capital.unwrap();

		assert_eq!(minted_capital[Ticker::Base], U256::from(317));
		assert_eq!(minted_capital[!Ticker::Base], U256::from(32));

		assert_eq!(
			minted_capital_accum[Ticker::Base] + minted_capital[Ticker::Base],
			U256::from(9_996 + 317)
		);
		assert_eq!(
			minted_capital_accum[!Ticker::Base] + minted_capital[!Ticker::Base],
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
			|_| true,
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
			|_| true,
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
			true
		})
		.unwrap();
		let minted_capital = minted_capital.unwrap();

		assert_eq!(minted_capital[Ticker::Base], U256::from(31623));
		assert_eq!(minted_capital[!Ticker::Base], U256::from(3163));

		assert_eq!(
			minted_capital_accum[Ticker::Base] + minted_capital[Ticker::Base],
			U256::from(9_996 + 31623)
		);
		assert_eq!(
			minted_capital_accum[!Ticker::Base] + minted_capital[!Ticker::Base],
			U256::from(1_000 + 3163)
		);
	}

	#[test]
	fn test_removing() {
		let (mut pool, _, id) = mint_pool();
		pool.mint(
			id,
			MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
			MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
			100,
			|_| true,
		)
		.unwrap();

		let (amounts_owed, _) = pool
			.burn(
				id,
				MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
				MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
				100,
			)
			.unwrap();

		assert_eq!(amounts_owed[Ticker::Base], U256::from(316));
		assert_eq!(amounts_owed[!Ticker::Base], U256::from(31));

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
			true
		})
		.unwrap();
		let minted_capital = minted_capital.unwrap();

		assert_eq!(minted_capital[Ticker::Base], U256::from(0));
		assert_eq!(minted_capital[!Ticker::Base], U256::from(2162));

		assert_eq!(
			minted_capital_accum[Ticker::Base] + minted_capital[Ticker::Base],
			U256::from(9_996)
		);
		assert_eq!(
			minted_capital_accum[!Ticker::Base] + minted_capital[!Ticker::Base],
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
				true
			},
		)
		.unwrap();
		let minted_capital = minted_capital.unwrap();

		assert_eq!(minted_capital[Ticker::Base], U256::from(0));
		assert_eq!(minted_capital[!Ticker::Base], U256::from(828011520));

		assert_eq!(
			minted_capital_accum[Ticker::Base] + minted_capital[Ticker::Base],
			U256::from(9_996)
		);
		assert_eq!(
			minted_capital_accum[!Ticker::Base] + minted_capital[!Ticker::Base],
			U256::from(1_000 + 828011520)
		);
	}

	#[test]
	fn test_mintick() {
		let (mut pool, minted_capital_accum, id) = mint_pool();
		let mut minted_capital = None;
		pool.mint(id, MIN_TICK_UNISWAP_MEDIUM, -23040, 10000, |minted| {
			minted_capital.replace(minted);
			true
		})
		.unwrap();
		let minted_capital = minted_capital.unwrap();

		assert_eq!(minted_capital[Ticker::Base], U256::from(0));
		assert_eq!(minted_capital[!Ticker::Base], U256::from(3161));

		assert_eq!(
			minted_capital_accum[Ticker::Base] + minted_capital[Ticker::Base],
			U256::from(9_996)
		);
		assert_eq!(
			minted_capital_accum[!Ticker::Base] + minted_capital[!Ticker::Base],
			U256::from(1_000 + 3161)
		);
	}

	#[test]
	fn test_removing_works_1() {
		let (mut pool, _, id) = mint_pool();
		let mut minted_capital = None;
		pool.mint(id, -46080, -46020, 10000, |minted| {
			minted_capital.replace(minted);
			true
		})
		.unwrap();

		let (returned_capital, fees_owed) = pool.burn(id, -46080, -46020, 10000).unwrap();

		// DIFF: Burn will have burnt the entire position so it will be deleted.
		assert_eq!(returned_capital[Ticker::Base], U256::from(0));
		assert_eq!(returned_capital[!Ticker::Base], U256::from(3));

		assert_eq!(fees_owed[Ticker::Base], 0);
		assert_eq!(fees_owed[!Ticker::Base], 0);

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
			H256([0xce; 32]),
			MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
			MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
			expandto18decimals(1).as_u128(),
			|_| true,
		)
		.unwrap();

		let SWAP_INPUT: u128 = expandto18decimals(1).as_u128();

		pool.swap::<BaseToPair>((SWAP_INPUT / 10).into());
		pool.swap::<PairToBase>((SWAP_INPUT / 100).into());

		match pool.burn(
			id,
			MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
			MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
			0,
		) {
			Err(PositionError::NonExistent) => {},
			_ => panic!("Expected NonExistent"),
		}

		let fees_owed = pool
			.mint(
				id,
				MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
				MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
				1,
				|_| true,
			)
			.unwrap();

		match (fees_owed[Ticker::Base], fees_owed[Ticker::Pair]) {
			(0, 0) => {},
			_ => panic!("Fees accrued are not zero"),
		}

		let tick = pool
			.positions
			.get(&(
				id,
				MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
				MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
			))
			.unwrap();
		assert_eq!(tick.liquidity, 1);
		assert_eq!(
			tick.last_fee_growth_inside[Ticker::Base],
			U256::from_dec_str("102084710076281216349243831104605583").unwrap()
		);
		assert_eq!(
			tick.last_fee_growth_inside[!Ticker::Base],
			U256::from_dec_str("10208471007628121634924383110460558").unwrap()
		);
		// assert_eq!(tick.fees_owed[Ticker::Base], 0);
		// assert_eq!(tick.fees_owed[!Ticker::Base], 0);

		let (returned_capital, fees_owed) = pool
			.burn(
				id,
				MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
				MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
				1,
			)
			.unwrap();

		// DIFF: Burn will have burnt the entire position so it will be deleted.
		assert_eq!(fees_owed[Ticker::Base], 0);
		assert_eq!(fees_owed[!Ticker::Base], 0);

		// This could be missing + fees_owed[Ticker::Base]
		assert_eq!(returned_capital[Ticker::Base], U256::from(3));
		assert_eq!(returned_capital[!Ticker::Base], U256::from(0));

		match pool.positions.get(&(
			id,
			MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
			MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
		)) {
			None => {},
			_ => panic!("Expected NonExistent Key"),
		}
	}

	pub const INITIALIZE_LIQUIDITY_AMOUNT: u128 = 2000000000000000000 as u128;

	// #Burn

	fn pool_initialized_zerotick(
		mut pool: PoolState,
	) -> (PoolState, enum_map::EnumMap<Ticker, Amount>, LiquidityProvider) {
		const ID: LiquidityProvider = H256([0xcf; 32]);
		let mut minted_capital = None;

		pool.mint(
			ID,
			MIN_TICK_UNISWAP_MEDIUM,
			MAX_TICK_UNISWAP_MEDIUM,
			INITIALIZE_LIQUIDITY_AMOUNT,
			|minted| {
				minted_capital.replace(minted);
				true
			},
		)
		.unwrap();
		let minted_capital = minted_capital.unwrap();

		(pool, minted_capital, ID)
	}

	// Medium Fee, tickSpacing = 12, 1:1 price
	fn mediumpool_initialized_zerotick(
	) -> (PoolState, enum_map::EnumMap<Ticker, Amount>, LiquidityProvider) {
		// fee_pips shall be one order of magnitude smaller than in the Uniswap pool (because
		// ONE_IN_PIPS is /10)
		let mut pool = PoolState::new(300, encodedprice1_1());
		pool_initialized_zerotick(pool)
	}

	fn checktickisclear(pool: &PoolState, tick: Tick) {
		match pool.liquidity_map.get(&tick) {
			None => {},
			_ => panic!("Expected NonExistent Key"),
		}
	}

	fn checkticknotclear(pool: &PoolState, tick: Tick) {
		match pool.liquidity_map.get(&tick) {
			None => panic!("Expected Key"),
			_ => {},
		}
	}

	// Own test
	#[test]
	fn test_multiple_burns() {
		let (mut pool, _, _id) = mediumpool_initialized_zerotick();
		// some activity that would make the ticks non-zero
		pool.mint(
			H256([0xce; 32]),
			MIN_TICK_UNISWAP_MEDIUM,
			MAX_TICK_UNISWAP_MEDIUM,
			expandto18decimals(1).as_u128(),
			|_| true,
		)
		.unwrap();

		pool.swap::<BaseToPair>(expandto18decimals(1));
		pool.swap::<PairToBase>(expandto18decimals(1));

		// Should be able to do only 1 burn (1000000000000000000 / 987654321000000000)

		pool.burn(
			H256([0xce; 32]),
			MIN_TICK_UNISWAP_MEDIUM,
			MAX_TICK_UNISWAP_MEDIUM,
			987654321000000000,
		)
		.unwrap();

		match pool.burn(
			H256([0xce; 32]),
			MIN_TICK_UNISWAP_MEDIUM,
			MAX_TICK_UNISWAP_MEDIUM,
			987654321000000000,
		) {
			Err(PositionError::Other(BurnError::PositionLacksLiquidity)) => {},
			_ => panic!("Expected InsufficientLiquidity"),
		}
	}

	#[test]
	fn test_notclearposition_ifnomoreliquidity() {
		let (mut pool, _, _id) = mediumpool_initialized_zerotick();
		// some activity that would make the ticks non-zero
		pool.mint(
			H256([0xce; 32]),
			MIN_TICK_UNISWAP_MEDIUM,
			MAX_TICK_UNISWAP_MEDIUM,
			expandto18decimals(1).as_u128(),
			|_| true,
		)
		.unwrap();

		pool.swap::<BaseToPair>(expandto18decimals(1));
		pool.swap::<PairToBase>(expandto18decimals(1));

		// Add a poke to update the fee growth and check it's value
		let (returned_capital, fees_owed) = pool
			.burn(H256([0xce; 32]), MIN_TICK_UNISWAP_MEDIUM, MAX_TICK_UNISWAP_MEDIUM, 0)
			.unwrap();

		assert_ne!(fees_owed[Ticker::Base], 0);
		assert_ne!(fees_owed[!Ticker::Base], 0);
		assert_eq!(returned_capital[Ticker::Base], U256::from(0));
		assert_eq!(returned_capital[!Ticker::Base], U256::from(0));

		let pos = pool
			.positions
			.get(&(H256([0xce; 32]), MIN_TICK_UNISWAP_MEDIUM, MAX_TICK_UNISWAP_MEDIUM))
			.unwrap();
		assert_eq!(
			pos.last_fee_growth_inside[Ticker::Base],
			U256::from_dec_str("340282366920938463463374607431768211").unwrap()
		);
		assert_eq!(
			pos.last_fee_growth_inside[!Ticker::Base],
			U256::from_dec_str("340282366920938463463374607431768211").unwrap()
		);

		let (returned_capital, fees_owed) = pool
			.burn(
				H256([0xce; 32]),
				MIN_TICK_UNISWAP_MEDIUM,
				MAX_TICK_UNISWAP_MEDIUM,
				expandto18decimals(1).as_u128(),
			)
			.unwrap();

		// DIFF: Burn will have burnt the entire position so it will be deleted.
		// Also, fees will already have been collected in the first burn.
		assert_eq!(fees_owed[Ticker::Base], 0);
		assert_eq!(fees_owed[!Ticker::Base], 0);

		// This could be missing + fees_owed[Ticker::Base]
		assert_ne!(returned_capital[Ticker::Base], U256::from(0));
		assert_ne!(returned_capital[!Ticker::Base], U256::from(0));

		match pool.positions.get(&(
			H256([0xce; 32]),
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
			id,
			MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
			MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
			1,
			|_| true,
		)
		.unwrap();

		pool.swap::<BaseToPair>(expandto18decimals(1));

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
			id,
			MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
			MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
			1,
			|_| true,
		)
		.unwrap();
		pool.mint(
			id,
			MIN_TICK_UNISWAP_MEDIUM + 2 * TICKSPACING_UNISWAP_MEDIUM,
			MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
			1,
			|_| true,
		)
		.unwrap();
		pool.swap::<BaseToPair>(expandto18decimals(1));

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
			id,
			MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
			MAX_TICK_UNISWAP_MEDIUM - TICKSPACING_UNISWAP_MEDIUM,
			1,
			|_| true,
		)
		.unwrap();
		pool.mint(
			id,
			MIN_TICK_UNISWAP_MEDIUM + TICKSPACING_UNISWAP_MEDIUM,
			MAX_TICK_UNISWAP_MEDIUM - 2 * TICKSPACING_UNISWAP_MEDIUM,
			1,
			|_| true,
		)
		.unwrap();
		pool.swap::<BaseToPair>(expandto18decimals(1));

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

	// Miscellaneous mint tests

	pub const TICKSPACING_UNISWAP_LOW: Tick = 10;
	pub const MIN_TICK_UNISWAP_LOW: Tick = -887220;
	pub const MAX_TICK_UNISWAP_LOW: Tick = -MIN_TICK_UNISWAP_LOW;

	// Low Fee, tickSpacing = 10, 1:1 price
	fn lowpool_initialized_zerotick(
	) -> (PoolState, enum_map::EnumMap<Ticker, Amount>, LiquidityProvider) {
		// TIckspaci
		let mut pool = PoolState::new(50, encodedprice1_1()); //	encodeSqrtPrice (1,1)
		pool_initialized_zerotick(pool)
	}

	#[test]
	fn test_mint_rightofcurrentprice() {
		let (mut pool, _, id) = lowpool_initialized_zerotick();

		let liquiditybefore = pool.current_liquidity.clone();

		let mut minted_capital = None;
		pool.mint(id, TICKSPACING_UNISWAP_LOW, 2 * TICKSPACING_UNISWAP_LOW, 1000, |minted| {
			minted_capital.replace(minted);
			true
		})
		.unwrap();
		let minted_capital = minted_capital.unwrap();

		assert!(pool.current_liquidity >= liquiditybefore);

		assert_eq!(minted_capital[Ticker::Base], U256::from(1));
		assert_eq!(minted_capital[!Ticker::Base], U256::from(0));
	}

	#[test]
	fn test_mint_leftofcurrentprice() {
		let (mut pool, _, id) = lowpool_initialized_zerotick();

		let liquiditybefore = pool.current_liquidity.clone();

		let mut minted_capital = None;
		pool.mint(id, -2 * TICKSPACING_UNISWAP_LOW, -TICKSPACING_UNISWAP_LOW, 1000, |minted| {
			minted_capital.replace(minted);
			true
		})
		.unwrap();
		let minted_capital = minted_capital.unwrap();

		assert!(pool.current_liquidity >= liquiditybefore);

		assert_eq!(minted_capital[Ticker::Base], U256::from(0));
		assert_eq!(minted_capital[!Ticker::Base], U256::from(1));
	}

	#[test]
	fn test_mint_withincurrentprice() {
		let (mut pool, _, id) = lowpool_initialized_zerotick();

		let liquiditybefore = pool.current_liquidity.clone();

		let mut minted_capital = None;
		pool.mint(id, -TICKSPACING_UNISWAP_LOW, TICKSPACING_UNISWAP_LOW, 1000, |minted| {
			minted_capital.replace(minted);
			true
		})
		.unwrap();
		let minted_capital = minted_capital.unwrap();

		assert!(pool.current_liquidity >= liquiditybefore);

		assert_eq!(minted_capital[Ticker::Base], U256::from(1));
		assert_eq!(minted_capital[!Ticker::Base], U256::from(1));
	}

	#[test]
	fn test_cannotremove_morethanposition() {
		let (mut pool, _, id) = lowpool_initialized_zerotick();

		pool.mint(
			id,
			-TICKSPACING_UNISWAP_LOW,
			TICKSPACING_UNISWAP_LOW,
			expandto18decimals(1).as_u128(),
			|_| true,
		)
		.unwrap();

		match pool.burn(
			id,
			-TICKSPACING_UNISWAP_LOW,
			TICKSPACING_UNISWAP_LOW,
			expandto18decimals(1).as_u128() + 1,
		) {
			Err(PositionError::Other(BurnError::PositionLacksLiquidity)) => {},
			_ => panic!("Should not be able to remove more than position"),
		}
	}

	#[test]
	fn test_collectfees_withincurrentprice() {
		let (mut pool, _, id) = lowpool_initialized_zerotick();

		pool.mint(
			id,
			-TICKSPACING_UNISWAP_LOW * 100,
			TICKSPACING_UNISWAP_LOW * 100,
			expandto18decimals(100).as_u128(),
			|_| true,
		)
		.unwrap();

		let liquiditybefore = pool.current_liquidity.clone();
		pool.swap::<BaseToPair>(expandto18decimals(1));

		assert!(pool.current_liquidity >= liquiditybefore);

		// Poke
		let (returned_capital, fees_owed) = pool
			.burn(id, -TICKSPACING_UNISWAP_LOW * 100, TICKSPACING_UNISWAP_LOW * 100, 0)
			.unwrap();

		assert_eq!(returned_capital[Ticker::Base], U256::from(0));
		assert_eq!(returned_capital[!Ticker::Base], U256::from(0));

		assert!(fees_owed[Ticker::Base] > 0);
		assert_eq!(fees_owed[!Ticker::Base], 0);
	}

	// Post initialize at medium fee

	#[test]
	fn test_initial_liquidity() {
		let (mut pool, _, id) = mediumpool_initialized_zerotick();
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
			|_| true,
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
			|_| true,
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
			|_| true,
		)
		.unwrap();
		assert_eq!(pool.current_liquidity, expandto18decimals(2).as_u128());
	}

	#[test]
	fn test_updates_exiting() {
		let (mut pool, _, id) = mediumpool_initialized_zerotick();
		assert_eq!(pool.current_liquidity, expandto18decimals(2).as_u128());

		pool.mint(id, 0, TICKSPACING_UNISWAP_MEDIUM, expandto18decimals(1).as_u128(), |_| true)
			.unwrap();
		assert_eq!(pool.current_liquidity, expandto18decimals(3).as_u128());

		// swap toward the left (just enough for the tick transition function to trigger)
		pool.swap::<BaseToPair>((1).into());

		assert_eq!(pool.current_tick, -1);
		assert_eq!(pool.current_liquidity, expandto18decimals(2).as_u128());
	}

	#[test]
	fn test_updates_entering() {
		let (mut pool, _, id) = mediumpool_initialized_zerotick();
		assert_eq!(pool.current_liquidity, expandto18decimals(2).as_u128());

		pool.mint(id, -TICKSPACING_UNISWAP_MEDIUM, 0, expandto18decimals(1).as_u128(), |_| true)
			.unwrap();
		assert_eq!(pool.current_liquidity, expandto18decimals(2).as_u128());

		// swap toward the left (just enough for the tick transition function to trigger)
		pool.swap::<BaseToPair>((1).into());

		assert_eq!(pool.current_tick, -1);
		assert_eq!(pool.current_liquidity, expandto18decimals(3).as_u128());
	}

	// Uniswap "limit orders"

	#[test]
	fn test_limitselling_basetopair_tick0thru1() {
		let (mut pool, _, id) = mediumpool_initialized_zerotick();
		let mut minted_capital = None;
		pool.mint(id, 0, 120, expandto18decimals(1).as_u128(), |minted| {
			minted_capital.replace(minted);
			true
		})
		.unwrap();
		let minted_capital = minted_capital.unwrap();

		assert_eq!(minted_capital[Ticker::Base], U256::from_dec_str("5981737760509663").unwrap());
		assert_eq!(minted_capital[!Ticker::Base], U256::from_dec_str("0").unwrap());

		// somebody takes the limit order
		pool.swap::<PairToBase>((U256::from_dec_str("2000000000000000000").unwrap()).into());

		let (burned, fees_owed) = pool.burn(id, 0, 120, expandto18decimals(1).as_u128()).unwrap();
		assert_eq!(burned[Ticker::Base], U256::from_dec_str("0").unwrap());
		assert_eq!(burned[!Ticker::Base], U256::from_dec_str("6017734268818165").unwrap());

		// DIFF: position fully burnt
		assert_eq!(fees_owed[Ticker::Base], 0);
		assert_eq!(fees_owed[!Ticker::Base], 18107525382602);

		match pool.burn(id, 0, 120, 1) {
			Err(PositionError::NonExistent) => {},
			_ => panic!("Expected NonExistent"),
		}

		assert!(pool.current_tick > 120)
	}

	#[test]
	fn test_limitselling_basetopair_tick0thru1_poke() {
		let (mut pool, _, id) = mediumpool_initialized_zerotick();
		let mut minted_capital = None;
		pool.mint(id, 0, 120, expandto18decimals(1).as_u128(), |minted| {
			minted_capital.replace(minted);
			true
		})
		.unwrap();
		let minted_capital = minted_capital.unwrap();

		assert_eq!(minted_capital[Ticker::Base], U256::from_dec_str("5981737760509663").unwrap());
		assert_eq!(minted_capital[!Ticker::Base], U256::from_dec_str("0").unwrap());

		// somebody takes the limit order
		pool.swap::<PairToBase>((U256::from_dec_str("2000000000000000000").unwrap()).into());

		let (burned, fees_owed) = pool.burn(id, 0, 120, 0).unwrap();
		assert_eq!(burned[Ticker::Base], U256::from_dec_str("0").unwrap());
		assert_eq!(burned[!Ticker::Base], U256::from_dec_str("0").unwrap());

		// DIFF: position fully burnt
		assert_eq!(fees_owed[Ticker::Base], 0);
		assert_eq!(fees_owed[!Ticker::Base], 18107525382602);

		let (burned, fees_owed) = pool.burn(id, 0, 120, expandto18decimals(1).as_u128()).unwrap();
		assert_eq!(burned[Ticker::Base], U256::from_dec_str("0").unwrap());
		assert_eq!(burned[!Ticker::Base], U256::from_dec_str("6017734268818165").unwrap());

		// DIFF: position fully burnt
		assert_eq!(fees_owed[Ticker::Base], 0);
		assert_eq!(fees_owed[!Ticker::Base], 0);

		assert!(pool.current_tick > 120)
	}

	#[test]
	fn test_limitselling_pairtobase_tick1thru0() {
		let (mut pool, _, id) = mediumpool_initialized_zerotick();
		let mut minted_capital = None;
		pool.mint(id, -120, 0, expandto18decimals(1).as_u128(), |minted| {
			minted_capital.replace(minted);
			true
		})
		.unwrap();
		let minted_capital = minted_capital.unwrap();

		assert_eq!(minted_capital[!Ticker::Base], U256::from_dec_str("5981737760509663").unwrap());
		assert_eq!(minted_capital[Ticker::Base], U256::from_dec_str("0").unwrap());

		// somebody takes the limit order
		pool.swap::<BaseToPair>((U256::from_dec_str("2000000000000000000").unwrap()).into());

		let (burned, fees_owed) = pool.burn(id, -120, 0, expandto18decimals(1).as_u128()).unwrap();
		assert_eq!(burned[!Ticker::Base], U256::from_dec_str("0").unwrap());
		assert_eq!(burned[Ticker::Base], U256::from_dec_str("6017734268818165").unwrap());

		// DIFF: position fully burnt
		assert_eq!(fees_owed[!Ticker::Base], 0);
		assert_eq!(fees_owed[Ticker::Base], 18107525382602);

		match pool.burn(id, -120, 0, 1) {
			Err(PositionError::NonExistent) => {},
			_ => panic!("Expected NonExistent"),
		}

		assert!(pool.current_tick < -120)
	}

	#[test]
	fn test_limitselling_pairtobase_tick1thru0_poke() {
		let (mut pool, _, id) = mediumpool_initialized_zerotick();
		let mut minted_capital = None;
		pool.mint(id, -120, 0, expandto18decimals(1).as_u128(), |minted| {
			minted_capital.replace(minted);
			true
		})
		.unwrap();
		let minted_capital = minted_capital.unwrap();

		assert_eq!(minted_capital[!Ticker::Base], U256::from_dec_str("5981737760509663").unwrap());
		assert_eq!(minted_capital[Ticker::Base], U256::from_dec_str("0").unwrap());

		// somebody takes the limit order
		pool.swap::<BaseToPair>((U256::from_dec_str("2000000000000000000").unwrap()).into());

		let (burned, fees_owed) = pool.burn(id, -120, 0, 0).unwrap();
		assert_eq!(burned[!Ticker::Base], U256::from_dec_str("0").unwrap());
		assert_eq!(burned[Ticker::Base], U256::from_dec_str("0").unwrap());

		assert_eq!(fees_owed[!Ticker::Base], 0);
		assert_eq!(fees_owed[Ticker::Base], 18107525382602);

		let (burned, fees_owed) = pool.burn(id, -120, 0, expandto18decimals(1).as_u128()).unwrap();
		assert_eq!(burned[!Ticker::Base], U256::from_dec_str("0").unwrap());
		assert_eq!(burned[Ticker::Base], U256::from_dec_str("6017734268818165").unwrap());

		// DIFF: position fully burnt
		assert_eq!(fees_owed[!Ticker::Base], 0);
		assert_eq!(fees_owed[Ticker::Base], 0);

		match pool.burn(id, -120, 0, 1) {
			Err(PositionError::NonExistent) => {},
			_ => panic!("Expected NonExistent"),
		}

		assert!(pool.current_tick < -120)
	}

	// #Collect

	// Low Fee, tickSpacing = 10, 1:1 price
	fn lowpool_initialized_one() -> (PoolState, enum_map::EnumMap<Ticker, Amount>, LiquidityProvider)
	{
		let mut pool = PoolState::new(50, encodedprice1_1());
		const ID: LiquidityProvider = H256([0xcf; 32]);
		let mut minted_amounts: enum_map::EnumMap<Ticker, Amount> = Default::default();
		(pool, minted_amounts, ID)
	}

	#[test]
	fn test_multiplelps() {
		let (mut pool, _, id) = lowpool_initialized_one();

		pool.mint(
			id,
			MIN_TICK_UNISWAP_LOW,
			MAX_TICK_UNISWAP_LOW,
			expandto18decimals(1).as_u128(),
			|_| true,
		)
		.unwrap();
		pool.mint(
			id,
			MIN_TICK_UNISWAP_LOW + TICKSPACING_UNISWAP_LOW,
			MAX_TICK_UNISWAP_LOW - TICKSPACING_UNISWAP_LOW,
			2000000000000000000,
			|_| true,
		)
		.unwrap();

		pool.swap::<BaseToPair>((expandto18decimals(1)).into());

		// poke positions
		let (burned, fees_owed) =
			pool.burn(id, MIN_TICK_UNISWAP_LOW, MAX_TICK_UNISWAP_LOW, 0).unwrap();

		assert_eq!(burned[Ticker::Base], U256::from_dec_str("0").unwrap());
		assert_eq!(burned[!Ticker::Base], U256::from_dec_str("0").unwrap());
		// NOTE: Fee_owed value 1 unit different than Uniswap because uniswap requires 4 loops to do
		// the swap instead of 1 causing the rounding to be different
		assert_eq!(fees_owed[Ticker::Base], 166666666666666 as u128);
		assert_eq!(fees_owed[!Ticker::Base], 0);

		let (burned, fees_owed) = pool
			.burn(
				id,
				MIN_TICK_UNISWAP_LOW + TICKSPACING_UNISWAP_LOW,
				MAX_TICK_UNISWAP_LOW - TICKSPACING_UNISWAP_LOW,
				0,
			)
			.unwrap();
		// NOTE: Fee_owed value 1 unit different than Uniswap because uniswap requires 4 loops to do
		// the swap instead of 1 causing the rounding to be different
		assert_eq!(fees_owed[Ticker::Base], 333333333333333 as u128);
		assert_eq!(fees_owed[!Ticker::Base], 0);
	}

	// type(uint128).max * 2**128 / 1e18
	// https://www.wolframalpha.com/input/?i=%282**128+-+1%29+*+2**128+%2F+1e18
	// U256::from_dec_str("115792089237316195423570985008687907852929702298719625575994").unwrap();

	// Works across large increases
	#[test]
	fn test_before_capbidn() {
		let (mut pool, _, id) = lowpool_initialized_one();
		pool.mint(
			id,
			MIN_TICK_UNISWAP_LOW,
			MAX_TICK_UNISWAP_LOW,
			expandto18decimals(1).as_u128(),
			|_| true,
		)
		.unwrap();

		pool.global_fee_growth[Ticker::Base] =
			U256::from_dec_str("115792089237316195423570985008687907852929702298719625575994")
				.unwrap();

		let (burned, fees_owed) =
			pool.burn(id, MIN_TICK_UNISWAP_LOW, MAX_TICK_UNISWAP_LOW, 0).unwrap();

		assert_eq!(burned[Ticker::Base], U256::from_dec_str("0").unwrap());
		assert_eq!(burned[!Ticker::Base], U256::from_dec_str("0").unwrap());

		assert_eq!(fees_owed[Ticker::Base], u128::MAX - 1);
		assert_eq!(fees_owed[!Ticker::Base], 0);
	}

	#[test]
	fn test_after_capbidn() {
		let (mut pool, _, id) = lowpool_initialized_one();
		pool.mint(
			id,
			MIN_TICK_UNISWAP_LOW,
			MAX_TICK_UNISWAP_LOW,
			expandto18decimals(1).as_u128(),
			|_| true,
		)
		.unwrap();

		pool.global_fee_growth[Ticker::Base] =
			U256::from_dec_str("115792089237316195423570985008687907852929702298719625575995")
				.unwrap();

		let (burned, fees_owed) =
			pool.burn(id, MIN_TICK_UNISWAP_LOW, MAX_TICK_UNISWAP_LOW, 0).unwrap();

		assert_eq!(burned[Ticker::Base], U256::from_dec_str("0").unwrap());
		assert_eq!(burned[!Ticker::Base], U256::from_dec_str("0").unwrap());

		assert_eq!(fees_owed[Ticker::Base], u128::MAX);
		assert_eq!(fees_owed[!Ticker::Base], 0);
	}

	#[test]
	fn test_wellafter_capbidn() {
		let (mut pool, _, id) = lowpool_initialized_one();
		pool.mint(
			id,
			MIN_TICK_UNISWAP_LOW,
			MAX_TICK_UNISWAP_LOW,
			expandto18decimals(1).as_u128(),
			|_| true,
		)
		.unwrap();

		pool.global_fee_growth[Ticker::Base] = U256::MAX;

		let (burned, fees_owed) =
			pool.burn(id, MIN_TICK_UNISWAP_LOW, MAX_TICK_UNISWAP_LOW, 0).unwrap();

		assert_eq!(burned[Ticker::Base], U256::from_dec_str("0").unwrap());
		assert_eq!(burned[!Ticker::Base], U256::from_dec_str("0").unwrap());

		assert_eq!(fees_owed[Ticker::Base], u128::MAX);
		assert_eq!(fees_owed[!Ticker::Base], 0);
	}

	// DIFF: pool.global_fee_growth won't overflow. We make it saturate.

	fn lowpool_initialized_setfees(
	) -> (PoolState, enum_map::EnumMap<Ticker, Amount>, LiquidityProvider) {
		let (mut pool, mut minted_amounts_accum, id) = lowpool_initialized_one();
		pool.global_fee_growth[Ticker::Base] = U256::MAX;
		pool.global_fee_growth[!Ticker::Base] = U256::MAX;

		let mut minted_capital = None;
		pool.mint(
			id,
			MIN_TICK_UNISWAP_LOW,
			MAX_TICK_UNISWAP_LOW,
			expandto18decimals(10).as_u128(),
			|minted| {
				minted_capital.replace(minted);
				true
			},
		)
		.unwrap();
		let minted_capital = minted_capital.unwrap();

		minted_amounts_accum[Ticker::Base] += minted_capital[Ticker::Base];
		minted_amounts_accum[!Ticker::Base] += minted_capital[!Ticker::Base];

		(pool, minted_amounts_accum, id)
	}

	#[test]
	fn test_base() {
		let (mut pool, _, id) = lowpool_initialized_setfees();

		pool.swap::<BaseToPair>((expandto18decimals(1)).into());

		assert_eq!(pool.global_fee_growth[Ticker::Base], U256::MAX);
		assert_eq!(pool.global_fee_growth[!Ticker::Base], U256::MAX);

		let (burned, fees_owed) =
			pool.burn(id, MIN_TICK_UNISWAP_LOW, MAX_TICK_UNISWAP_LOW, 0).unwrap();

		// DIFF: no fees accrued
		assert_eq!(fees_owed[Ticker::Base], 0);
		assert_eq!(fees_owed[!Ticker::Base], 0);
	}

	#[test]
	fn test_pair() {
		let (mut pool, _, id) = lowpool_initialized_setfees();

		pool.swap::<PairToBase>((expandto18decimals(1)).into());

		assert_eq!(pool.global_fee_growth[Ticker::Base], U256::MAX);
		assert_eq!(pool.global_fee_growth[!Ticker::Base], U256::MAX);

		let (_, fees_owed) = pool.burn(id, MIN_TICK_UNISWAP_LOW, MAX_TICK_UNISWAP_LOW, 0).unwrap();

		// DIFF: no fees accrued
		assert_eq!(fees_owed[Ticker::Base], 0 as u128);
		assert_eq!(fees_owed[!Ticker::Base], 0);
	}

	// Skipped more fee protocol tests

	// #Tickspacing

	// Medium Fee, tickSpacing = 12, 1:1 price
	fn mediumpool_initialized_nomint(
	) -> (PoolState, enum_map::EnumMap<Ticker, Amount>, LiquidityProvider) {
		// fee_pips shall be one order of magnitude smaller than in the Uniswap pool (because
		// ONE_IN_PIPS is /10)
		let mut pool = PoolState::new(300, encodedprice1_1());
		const ID: LiquidityProvider = H256([0xcf; 32]);
		let mut minted_amounts: enum_map::EnumMap<Ticker, Amount> = Default::default();
		(pool, minted_amounts, ID)
	}

	// DIFF: We have a tickspacing of 1, which means we will never have issues with it.
	#[test]
	fn test_tickspacing() {
		let (mut pool, _, id) = mediumpool_initialized_nomint();
		pool.mint(id, -6, 6, 1, |_| true).unwrap();
		pool.mint(id, -12, 12, 1, |_| true).unwrap();
		pool.mint(id, -144, 120, 1, |_| true).unwrap();
		pool.mint(id, -144, -120, 1, |_| true).unwrap();
	}

	#[test]
	fn test_swapping_gaps_pairtobase() {
		let (mut pool, _, id) = mediumpool_initialized_nomint();
		pool.mint(id, 120000, 121200, 250000000000000000, |_| true).unwrap();
		pool.swap::<PairToBase>((expandto18decimals(1)).into());
		let (returned_capital, fees_owed) =
			pool.burn(id, 120000, 121200, 250000000000000000).unwrap();

		assert_eq!(returned_capital[Ticker::Base], U256::from_dec_str("30027458295511").unwrap());
		assert_eq!(
			returned_capital[!Ticker::Base],
			U256::from_dec_str("996999999999999999").unwrap()
		);

		assert_eq!(fees_owed[Ticker::Base], 0);
		assert!(fees_owed[!Ticker::Base] > 0);

		assert_eq!(pool.current_tick, 120196)
	}

	#[test]
	fn test_swapping_gaps_basetopair() {
		let (mut pool, _, id) = mediumpool_initialized_nomint();
		pool.mint(id, -121200, -120000, 250000000000000000, |_| true).unwrap();
		pool.swap::<BaseToPair>((expandto18decimals(1)).into());
		let (returned_capital, fees_owed) =
			pool.burn(id, -121200, -120000, 250000000000000000).unwrap();

		assert_eq!(
			returned_capital[Ticker::Base],
			U256::from_dec_str("996999999999999999").unwrap()
		);
		assert_eq!(returned_capital[!Ticker::Base], U256::from_dec_str("30027458295511").unwrap());

		assert_eq!(fees_owed[!Ticker::Base], 0);
		assert!(fees_owed[Ticker::Base] > 0);

		assert_eq!(pool.current_tick, -120197)
	}

	#[test]
	fn test_cannot_run_ticktransition_twice() {
		const ID: LiquidityProvider = H256([0xcf; 32]);

		let p0 = PoolState::sqrt_price_at_tick(-24081) + 1;
		let mut pool = PoolState::new(300, p0);
		assert_eq!(pool.current_liquidity, 0);
		assert_eq!(pool.current_tick, -24081);

		// add a bunch of liquidity around current price
		pool.mint(ID, -24082, -24080, expandto18decimals(1000).as_u128() as u128, |_| true)
			.unwrap();
		assert_eq!(pool.current_liquidity, expandto18decimals(1000).as_u128() as u128);

		pool.mint(ID, -24082, -24081, expandto18decimals(1000).as_u128() as u128, |_| true)
			.unwrap();
		assert_eq!(pool.current_liquidity, expandto18decimals(1000).as_u128() as u128);

		// check the math works out to moving the price down 1, sending no amount out, and having
		// some amount remaining
		let amount_swapped = pool.swap::<BaseToPair>((U256::from_dec_str("3").unwrap()).into());
		assert_eq!(amount_swapped, U256::from_dec_str("0").unwrap());

		assert_eq!(pool.current_tick, -24082);
		assert_eq!(pool.current_sqrt_price, p0 - 1);
		assert_eq!(pool.current_liquidity, 2000000000000000000000 as u128);
	}

	///////////////////////////////////////////////////////////
	///               TEST SQRTPRICE MATH                  ////
	///////////////////////////////////////////////////////////
	#[test]
	#[should_panic]
	fn test_frominput_fails_zero() {
		// test PairToBase next_sqrt_price_from_input_amount
		PairToBase::next_sqrt_price_from_input_amount(
			U256::from_dec_str("0").unwrap(),
			0,
			expandto18decimals(1) / 10,
		);
	}
	#[test]
	#[should_panic]
	fn test_frominput_fails_liqzero() {
		BaseToPair::next_sqrt_price_from_input_amount(
			U256::from_dec_str("1").unwrap(),
			0,
			expandto18decimals(1) / 10,
		);
	}

	// TODO: To fix if we tighten up the data types
	#[test]
	#[should_panic]
	fn test_frominput_fails_inputoverflow() {
		PairToBase::next_sqrt_price_from_input_amount(
			U256::from_dec_str("1461501637330902918203684832716283019655932542975").unwrap(), /* 2^160-1 */
			1024,
			U256::from_dec_str("1461501637330902918203684832716283019655932542976").unwrap(), /* 2^160 */
		);
	}
	#[test]
	#[should_panic]
	fn test_frominput_fails_anyinputoverflow() {
		PairToBase::next_sqrt_price_from_input_amount(
			U256::from_dec_str("1").unwrap(),
			1,
			U256::from_dec_str(
				"57896044618658097711785492504343953926634992332820282019728792003956564819968",
			)
			.unwrap(), //2^255
		);
	}

	#[test]
	fn test_frominput_zeroamount_basetopair() {
		let price = BaseToPair::next_sqrt_price_from_input_amount(
			encodedprice1_1(),
			expandto18decimals(1).as_u128(),
			U256::from_dec_str("0").unwrap(),
		);
		assert_eq!(price, encodedprice1_1());
	}

	#[test]
	fn test_frominput_zeroamount_pairtobase() {
		let price = PairToBase::next_sqrt_price_from_input_amount(
			encodedprice1_1(),
			expandto18decimals(1).as_u128(),
			U256::from_dec_str("0").unwrap(),
		);
		assert_eq!(price, encodedprice1_1());
	}

	#[test]
	fn test_maxamounts_minprice() {
		let sqrtP: U256 =
			U256::from_dec_str("1461501637330902918203684832716283019655932542976").unwrap();
		let liquidity: u128 = u128::MAX;
		let maxamount_nooverflow = U256::MAX - (liquidity << 96); // sqrtP)

		let price = BaseToPair::next_sqrt_price_from_input_amount(
			sqrtP, //2^96
			liquidity,
			maxamount_nooverflow,
		);

		assert_eq!(price, 1.into());
	}

	#[test]
	fn test_frominput_inputamount_pair() {
		let price = PairToBase::next_sqrt_price_from_input_amount(
			encodedprice1_1(), //encodePriceSqrt(1, 1)
			expandto18decimals(1).as_u128(),
			expandto18decimals(1) / 10,
		);
		assert_eq!(price, U256::from_dec_str("87150978765690771352898345369").unwrap());
	}

	#[test]
	fn test_frominput_inputamount_base() {
		let price = BaseToPair::next_sqrt_price_from_input_amount(
			encodedprice1_1(), //encodePriceSqrt(1, 1)
			expandto18decimals(1).as_u128(),
			expandto18decimals(1) / 10,
		);
		assert_eq!(price, U256::from_dec_str("72025602285694852357767227579").unwrap());
	}

	#[test]
	fn test_frominput_amountinmaxuint96_base() {
		let price = BaseToPair::next_sqrt_price_from_input_amount(
			encodedprice1_1(), //encodePriceSqrt(1, 1)
			expandto18decimals(10).as_u128(),
			U256::from_dec_str("1267650600228229401496703205376").unwrap(), // 2**100
		);
		assert_eq!(price, U256::from_dec_str("624999999995069620").unwrap());
	}

	#[test]
	fn test_frominput_amountinmaxuint96_pair() {
		let price = BaseToPair::next_sqrt_price_from_input_amount(
			encodedprice1_1(), //encodePriceSqrt(1, 1)
			1 as u128,
			U256::MAX / 2,
		);
		assert_eq!(price, U256::from_dec_str("1").unwrap());
	}

	// Skip get amount from output

	// #getAmount0Delta
	fn encodedprice1_1() -> U256 {
		U256::from_dec_str("79228162514264337593543950336").unwrap()
	}
	fn encodedprice2_1() -> U256 {
		U256::from_dec_str("112045541949572287496682733568").unwrap()
	}
	fn encodedprice121_100() -> U256 {
		U256::from_dec_str("87150978765690771352898345369").unwrap()
	}
	fn expandto18decimals(amount: u128) -> U256 {
		U256::from(amount) * U256::from(10).pow(U256::from_dec_str("18").unwrap())
	}

	#[test]
	fn test_expanded() {
		assert_eq!(expandto18decimals(1), expandto18decimals(1));
	}

	#[test]
	fn test_0_if_liquidity_0() {
		assert_eq!(
			PoolState::base_amount_delta_ceil(encodedprice1_1(), encodedprice2_1(), 0),
			U256::from(0)
		);
	}

	#[test]
	fn test_price1_121() {
		assert_eq!(
			PoolState::base_amount_delta_ceil(
				encodedprice1_1(),
				encodedprice121_100(),
				expandto18decimals(1).as_u128()
			),
			U256::from_dec_str("90909090909090910").unwrap()
		);

		assert_eq!(
			PoolState::base_amount_delta_floor(
				encodedprice1_1(),
				encodedprice121_100(),
				expandto18decimals(1).as_u128()
			),
			U256::from_dec_str("90909090909090909").unwrap()
		);
	}

	#[test]
	fn test_overflow() {
		assert_eq!(
			PoolState::base_amount_delta_ceil(
				U256::from_dec_str("2787593149816327892691964784081045188247552").unwrap(),
				U256::from_dec_str("22300745198530623141535718272648361505980416").unwrap(),
				expandto18decimals(1).as_u128(),
			),
			PoolState::base_amount_delta_floor(
				U256::from_dec_str("2787593149816327892691964784081045188247552").unwrap(),
				U256::from_dec_str("22300745198530623141535718272648361505980416").unwrap(),
				expandto18decimals(1).as_u128(),
			) + 1,
		);
	}

	// #getAmount1Delta

	#[test]
	fn test_0_if_liquidity_0_pair() {
		assert_eq!(
			PoolState::pair_amount_delta_ceil(encodedprice1_1(), encodedprice2_1(), 0),
			U256::from(0)
		);
	}

	#[test]
	fn test_price1_121_pair() {
		assert_eq!(
			PoolState::pair_amount_delta_ceil(
				encodedprice1_1(),
				encodedprice121_100(),
				expandto18decimals(1).as_u128()
			),
			expandto18decimals(1) / 10
		);

		assert_eq!(
			PoolState::pair_amount_delta_floor(
				encodedprice1_1(),
				encodedprice121_100(),
				expandto18decimals(1).as_u128()
			),
			expandto18decimals(1) / 10 - 1
		);
	}

	// Swap computation
	#[test]
	fn test_sqrtoverflows() {
		let sqrtP =
			U256::from_dec_str("1025574284609383690408304870162715216695788925244").unwrap();
		let liquidity = 50015962439936049619261659728067971248 as u128;
		let sqrtQ = BaseToPair::next_sqrt_price_from_input_amount(
			sqrtP,
			liquidity,
			U256::from_dec_str("406").unwrap(),
		);
		assert_eq!(
			sqrtQ,
			U256::from_dec_str("1025574284609383582644711336373707553698163132913").unwrap()
		);

		assert_eq!(
			PoolState::base_amount_delta_ceil(sqrtQ, sqrtP, liquidity as u128),
			U256::from_dec_str("406").unwrap()
		);
	}

	///////////////////////////////////////////////////////////
	///                  TEST SWAPMATH                     ////
	///////////////////////////////////////////////////////////

	// computeSwapStep

	// We cannot really fake the state of the pool to test this because we would need to mint a
	// tick equivalent to desired sqrtPriceTarget but:
	// sqrt_price_at_tick(tick_at_sqrt_price(sqrtPriceTarget)) != sqrtPriceTarget, due to the
	// prices being between ticks - and therefore converting them to the closes tick.
	//#[test]
	fn test_amountcapped_pairtobase_fail() {
		let mut pool = PoolState::new(60, encodedprice1_1());
		const ID: LiquidityProvider = H256([0xcf; 32]);

		let mut minted_capital = None;

		pool.mint(
			ID,
			PoolState::tick_at_sqrt_price(encodedprice1_1()),
			PoolState::tick_at_sqrt_price(
				U256::from_dec_str("79623317895830914510487008059").unwrap(),
			),
			expandto18decimals(2).as_u128(),
			|minted| {
				minted_capital.replace(minted);
				true
			},
		)
		.unwrap();

		let minted_capital = minted_capital.unwrap();
		println!(
			"upper tick: {:?}",
			PoolState::tick_at_sqrt_price(
				U256::from_dec_str("79623317895830914510487008059").unwrap(),
			)
		);
		println!(
			"Reversed target price: {:?}",
			PoolState::sqrt_price_at_tick(PoolState::tick_at_sqrt_price(
				U256::from_dec_str("79623317895830914510487008059").unwrap(),
			))
		);
		// Swap to the right towards price target
		let amount_swapped = pool.swap::<PairToBase>((expandto18decimals(1)).into());
		println!("Amount swapped: {:?}", amount_swapped);
	}

	// Fake computeswapstep => Stripped down version of the real swap
	// TODO: Consider refactoring real AMM to be able to easily test this.
	// NOTE: Using ONE_IN_PIPS_UNISWAP here to match the tests. otherwise we would need decimals for
	// the fee value
	const ONE_IN_PIPS_UNISWAP: u32 = 1000000;

	fn compute_swapstep<SD: SwapDirection>(
		current_sqrt_price: SqrtPriceQ64F96,
		sqrt_ratio_target: SqrtPriceQ64F96,
		liquidity: Liquidity,
		mut amount: Amount,
		fee: u32,
	) -> (Amount, Amount, SqrtPriceQ64F96, U256) {
		let mut total_amount_out = Amount::zero();

		let amount_minus_fees = mul_div_floor(
			amount,
			U256::from(ONE_IN_PIPS_UNISWAP - fee),
			U256::from(ONE_IN_PIPS_UNISWAP),
		); // This cannot overflow as we bound fee_pips to <= ONE_IN_PIPS/2 (TODO)

		let amount_required_to_reach_target =
			SD::input_amount_delta_ceil(current_sqrt_price, sqrt_ratio_target, liquidity);

		let sqrt_ratio_next = if amount_minus_fees >= amount_required_to_reach_target {
			sqrt_ratio_target
		} else {
			assert!(liquidity != 0);
			SD::next_sqrt_price_from_input_amount(current_sqrt_price, liquidity, amount_minus_fees)
		};

		// Cannot overflow as if the swap traversed all ticks (MIN_TICK to MAX_TICK
		// (inclusive)), assuming the maximum possible liquidity, total_amount_out would still
		// be below U256::MAX (See test `output_amounts_bounded`)
		total_amount_out +=
			SD::output_amount_delta_floor(current_sqrt_price, sqrt_ratio_next, liquidity);

		// next_sqrt_price_from_input_amount rounds so this maybe true even though
		// amount_minus_fees < amount_required_to_reach_target (TODO Prove)
		let mut fees: U256 = Default::default();
		if sqrt_ratio_next == sqrt_ratio_target {
			// Will not overflow as fee_pips <= ONE_IN_PIPS / 2
			fees = mul_div_ceil(
				amount_required_to_reach_target,
				U256::from(fee),
				U256::from(ONE_IN_PIPS_UNISWAP - fee),
			);

			// TODO: Prove these don't underflow
			amount -= amount_required_to_reach_target;
			amount -= fees;
			return (amount_required_to_reach_target, total_amount_out, sqrt_ratio_next, fees)
		} else {
			let amount_in =
				SD::input_amount_delta_ceil(current_sqrt_price, sqrt_ratio_next, liquidity);
			// Will not underflow due to rounding in flavor of the pool of both sqrt_ratio_next
			// and amount_in. (TODO: Prove)
			fees = amount - amount_in;
			return (amount_in, total_amount_out, sqrt_ratio_next, fees)
		};
	}

	#[test]
	fn test_amountcapped_pairtobase() {
		let price = encodedprice1_1();
		let amount = expandto18decimals(1);
		let price_target = U256::from_dec_str("79623317895830914510487008059").unwrap();
		let liquidity = expandto18decimals(2).as_u128();
		let (amount_in, amount_out, sqrt_ratio_next, fees) =
			compute_swapstep::<PairToBase>(price, price_target, liquidity, amount, 600);

		assert_eq!(amount_in, U256::from_dec_str("9975124224178055").unwrap());
		assert_eq!(fees, U256::from_dec_str("5988667735148").unwrap());
		assert_eq!(amount_out, U256::from_dec_str("9925619580021728").unwrap());
		assert!(amount_in + fees < amount);

		let price_after_input_amount =
			PoolState::next_sqrt_price_from_pair_input(price, liquidity, amount);

		assert_eq!(sqrt_ratio_next, price_target);
		assert!(sqrt_ratio_next < price_after_input_amount);
	}

	// Skip amountout test

	#[test]
	fn test_amountin_spent_pairtobase() {
		let price = encodedprice1_1();
		let price_target = U256::from_dec_str("792281625142643375935439503360").unwrap();
		let liquidity = expandto18decimals(2).as_u128();
		let amount = expandto18decimals(1);
		let (amount_in, amount_out, sqrt_ratio_next, fees) =
			compute_swapstep::<PairToBase>(price, price_target, liquidity, amount, 600);

		assert_eq!(amount_in, U256::from_dec_str("999400000000000000").unwrap());
		assert_eq!(fees, U256::from_dec_str("600000000000000").unwrap());
		assert_eq!(amount_out, U256::from_dec_str("666399946655997866").unwrap());
		assert_eq!(amount_in + fees, amount);

		let price_after_input_amount =
			PoolState::next_sqrt_price_from_pair_input(price, liquidity, amount - fees);

		assert!(sqrt_ratio_next < price_target);
		assert_eq!(sqrt_ratio_next, price_after_input_amount);
	}

	#[test]
	fn test_targetprice1_partialinput() {
		let (amount_in, amount_out, sqrt_ratio_next, fees) = compute_swapstep::<BaseToPair>(
			U256::from_dec_str("2").unwrap(),
			U256::from_dec_str("1").unwrap(),
			1 as u128,
			U256::from_dec_str("3915081100057732413702495386755767").unwrap(),
			1,
		);
		assert_eq!(amount_in, U256::from_dec_str("39614081257132168796771975168").unwrap());
		assert_eq!(fees, U256::from_dec_str("39614120871253040049813").unwrap());
		assert!(
			amount_in + fees < U256::from_dec_str("3915081100057732413702495386755767").unwrap()
		);
		assert_eq!(amount_out, U256::from(0));
		assert_eq!(sqrt_ratio_next, U256::from_dec_str("1").unwrap());
	}

	#[test]
	fn test_entireinput_asfee() {
		let (amount_in, amount_out, sqrt_ratio_next, fees) = compute_swapstep::<PairToBase>(
			U256::from_dec_str("2413").unwrap(),
			U256::from_dec_str("79887613182836312").unwrap(),
			1985041575832132834610021537970 as u128,
			U256::from_dec_str("10").unwrap(),
			1872,
		);
		assert_eq!(amount_in, U256::from_dec_str("0").unwrap());
		assert_eq!(fees, U256::from_dec_str("10").unwrap());
		assert_eq!(amount_out, U256::from_dec_str("0").unwrap());
		assert_eq!(sqrt_ratio_next, U256::from_dec_str("2413").unwrap());
	}

	///////////////////////////////////////////////////////////
	///                  ADDED TESTS                       ////
	///////////////////////////////////////////////////////////

	// Add some more tests for fees_owed collecting

	// Previous tests using mint as a poke and to collect fees.

	#[test]
	fn test_limitselling_basetopair_tick0thru1_mint() {
		let (mut pool, _, id) = mediumpool_initialized_zerotick();
		let mut minted_capital = None;
		pool.mint(id, 0, 120, expandto18decimals(1).as_u128(), |minted| {
			minted_capital.replace(minted);
			true
		})
		.unwrap();
		let minted_capital = minted_capital.unwrap();

		assert_eq!(minted_capital[Ticker::Base], U256::from_dec_str("5981737760509663").unwrap());
		assert_eq!(minted_capital[!Ticker::Base], U256::from_dec_str("0").unwrap());

		// somebody takes the limit order
		pool.swap::<PairToBase>((U256::from_dec_str("2000000000000000000").unwrap()).into());

		let fees_owed = pool.mint(id, 0, 120, 1, |_| true).unwrap();

		assert_eq!(fees_owed[Ticker::Base], 0);
		assert_eq!(fees_owed[!Ticker::Base], 18107525382602);

		let fees_owed = pool.mint(id, 0, 120, 1, |_| true).unwrap();
		assert_eq!(fees_owed[Ticker::Base], 0);
		assert_eq!(fees_owed[!Ticker::Base], 0);

		let (burned, fees_owed) = pool.burn(id, 0, 120, expandto18decimals(1).as_u128()).unwrap();
		assert_eq!(burned[Ticker::Base], U256::from_dec_str("0").unwrap());
		assert_eq!(burned[!Ticker::Base], U256::from_dec_str("6017734268818165").unwrap());

		// DIFF: position fully burnt
		assert_eq!(fees_owed[Ticker::Base], 0);
		assert_eq!(fees_owed[!Ticker::Base], 0);

		assert!(pool.current_tick > 120)
	}

	#[test]
	fn test_limitselling_paior_tick1thru0_mint() {
		let (mut pool, _, id) = mediumpool_initialized_zerotick();
		let mut minted_capital = None;
		pool.mint(id, -120, 0, expandto18decimals(1).as_u128(), |minted| {
			minted_capital.replace(minted);
			true
		})
		.unwrap();
		let minted_capital = minted_capital.unwrap();

		assert_eq!(minted_capital[!Ticker::Base], U256::from_dec_str("5981737760509663").unwrap());
		assert_eq!(minted_capital[Ticker::Base], U256::from_dec_str("0").unwrap());

		// somebody takes the limit order
		pool.swap::<BaseToPair>((U256::from_dec_str("2000000000000000000").unwrap()).into());

		let fees_owed = pool.mint(id, -120, 0, 1, |_| true).unwrap();

		assert_eq!(fees_owed[!Ticker::Base], 0);
		assert_eq!(fees_owed[Ticker::Base], 18107525382602);

		let fees_owed = pool.mint(id, -120, 0, 1, |_| true).unwrap();
		assert_eq!(fees_owed[!Ticker::Base], 0);
		assert_eq!(fees_owed[Ticker::Base], 0);

		let (burned, fees_owed) = pool.burn(id, -120, 0, expandto18decimals(1).as_u128()).unwrap();
		assert_eq!(burned[!Ticker::Base], U256::from_dec_str("0").unwrap());
		assert_eq!(burned[Ticker::Base], U256::from_dec_str("6017734268818165").unwrap());

		// DIFF: position fully burnt
		assert_eq!(fees_owed[!Ticker::Base], 0);
		assert_eq!(fees_owed[Ticker::Base], 0);

		assert!(pool.current_tick < -120)
	}
}
