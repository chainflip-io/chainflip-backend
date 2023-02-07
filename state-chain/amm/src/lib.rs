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
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod tests;

use sp_std::collections::btree_map::BTreeMap;

use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

use cf_primitives::{
	liquidity::{AmountU256, Liquidity, PoolAssetMap, PoolSide, Tick},
	AccountId, AmmRange,
};
use sp_core::{U256, U512};

/// sqrt(Price) in amm exchange Pool. Q64.96 numerical type.
pub type SqrtPriceQ64F96 = U256;

/// Q128.128 numerical type use to record Fee.
pub type FeeGrowthQ128F128 = U256;

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

/// Minimum resolution for fee is 0.01 of a Basis Point: 0.0001%. Maximum is 50%.
const ONE_IN_HUNDREDTH_BIPS: u32 = 1000000;

pub const MAX_FEE_100TH_BIPS: u32 = ONE_IN_HUNDREDTH_BIPS / 2;

#[derive(Copy, Clone, Debug, TypeInfo, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
struct Position {
	liquidity: Liquidity,
	last_fee_growth_inside: PoolAssetMap<FeeGrowthQ128F128>,
}

impl Position {
	fn update_fees_owed(
		&mut self,
		pool_state: &PoolState,
		lower_tick: Tick,
		lower_info: &TickInfo,
		upper_tick: Tick,
		upper_info: &TickInfo,
	) -> PoolAssetMap<u128> {
		let fee_growth_inside = PoolAssetMap::new_from_fn(|side| {
			let fee_growth_below = if pool_state.current_tick < lower_tick {
				pool_state.global_fee_growth[side] - lower_info.fee_growth_outside[side]
			} else {
				lower_info.fee_growth_outside[side]
			};

			let fee_growth_above = if pool_state.current_tick < upper_tick {
				upper_info.fee_growth_outside[side]
			} else {
				pool_state.global_fee_growth[side] - upper_info.fee_growth_outside[side]
			};

			pool_state.global_fee_growth[side] - fee_growth_below - fee_growth_above
		});
		// DIFF: This behaviour is different than Uniswap's. We saturate fees_owed instead of
		// overflowing
		let fees_owed = PoolAssetMap::new_from_fn(|side| {
			/*
				Proof that `mul_div` does not overflow:
				Note position.liqiudity: u128
				U512::one() << 128 > u128::MAX
			*/
			mul_div_floor(
				fee_growth_inside[side] - self.last_fee_growth_inside[side],
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
	) -> PoolAssetMap<u128> {
		let fees_owed =
			self.update_fees_owed(pool_state, lower_tick, lower_info, upper_tick, upper_info);
		self.liquidity = new_liquidity;
		fees_owed
	}
}

#[derive(Copy, Clone, Debug, Default, TypeInfo, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct TickInfo {
	liquidity_delta: i128,
	liquidity_gross: u128,
	fee_growth_outside: PoolAssetMap<FeeGrowthQ128F128>,
}

trait SwapDirection {
	const INPUT_SIDE: PoolSide;

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
	) -> AmountU256;
	/// Calculates the swap output amount needed to move the current price given the specified
	/// amount of liquidity
	fn output_amount_delta_floor(
		from: SqrtPriceQ64F96,
		to: SqrtPriceQ64F96,
		liquidity: Liquidity,
	) -> AmountU256;

	/// Calculates where the current price will be after a swap of amount given the current price
	/// and a specific amount of liquidity
	fn next_sqrt_price_from_input_amount(
		sqrt_ratio_current: SqrtPriceQ64F96,
		liquidity: Liquidity,
		amount: AmountU256,
	) -> SqrtPriceQ64F96;

	/// For a given tick calculates the change in current liquidity when that tick is crossed
	fn liquidity_delta_on_crossing_tick(tick_liquidity: &TickInfo) -> i128;

	/// The current tick is always the closest tick less than the current_sqrt_price
	fn current_tick_after_crossing_target_tick(target_tick: Tick) -> Tick;
}

pub struct Asset0ToAsset1 {}
impl SwapDirection for Asset0ToAsset1 {
	const INPUT_SIDE: PoolSide = PoolSide::Asset0;

	fn target_tick(
		current_tick: Tick,
		liquidity_map: &mut BTreeMap<Tick, TickInfo>,
	) -> Option<(&Tick, &mut TickInfo)> {
		debug_assert!(liquidity_map.contains_key(&MIN_TICK));
		if current_tick >= MIN_TICK {
			Some(
				liquidity_map
					.range_mut(..=current_tick)
					.next_back()
					.expect("MIN_TICK's TickInfo always exists."),
			)
		} else {
			debug_assert_eq!(current_tick, Self::current_tick_after_crossing_target_tick(MIN_TICK));
			None
		}
	}

	fn input_amount_delta_ceil(
		current: SqrtPriceQ64F96,
		target: SqrtPriceQ64F96,
		liquidity: Liquidity,
	) -> AmountU256 {
		PoolState::asset_0_amount_delta_ceil(target, current, liquidity)
	}

	fn output_amount_delta_floor(
		current: SqrtPriceQ64F96,
		target: SqrtPriceQ64F96,
		liquidity: Liquidity,
	) -> AmountU256 {
		PoolState::asset_1_amount_delta_floor(target, current, liquidity)
	}

	fn next_sqrt_price_from_input_amount(
		sqrt_ratio_current: SqrtPriceQ64F96,
		liquidity: Liquidity,
		amount: AmountU256,
	) -> SqrtPriceQ64F96 {
		PoolState::next_sqrt_price_from_asset_0_input(sqrt_ratio_current, liquidity, amount)
	}

	fn liquidity_delta_on_crossing_tick(tick_liquidity: &TickInfo) -> i128 {
		-tick_liquidity.liquidity_delta
	}

	fn current_tick_after_crossing_target_tick(target_tick: Tick) -> Tick {
		target_tick - 1
	}
}

pub struct Asset1ToAsset0 {}
impl SwapDirection for Asset1ToAsset0 {
	const INPUT_SIDE: PoolSide = PoolSide::Asset1;

	fn target_tick(
		current_tick: Tick,
		liquidity_map: &mut BTreeMap<Tick, TickInfo>,
	) -> Option<(&Tick, &mut TickInfo)> {
		debug_assert!(liquidity_map.contains_key(&MAX_TICK));
		if current_tick < MAX_TICK {
			Some(
				liquidity_map
					.range_mut(current_tick + 1..)
					.next()
					.expect("MAX_TICK's TickInfo always exists."),
			)
		} else {
			debug_assert_eq!(current_tick, Self::current_tick_after_crossing_target_tick(MAX_TICK));
			None
		}
	}

	fn input_amount_delta_ceil(
		current: SqrtPriceQ64F96,
		target: SqrtPriceQ64F96,
		liquidity: Liquidity,
	) -> AmountU256 {
		PoolState::asset_1_amount_delta_ceil(current, target, liquidity)
	}

	fn output_amount_delta_floor(
		current: SqrtPriceQ64F96,
		target: SqrtPriceQ64F96,
		liquidity: Liquidity,
	) -> AmountU256 {
		PoolState::asset_0_amount_delta_floor(current, target, liquidity)
	}

	fn next_sqrt_price_from_input_amount(
		sqrt_ratio_current: SqrtPriceQ64F96,
		liquidity: Liquidity,
		amount: AmountU256,
	) -> SqrtPriceQ64F96 {
		PoolState::next_sqrt_price_from_asset_1_input(sqrt_ratio_current, liquidity, amount)
	}

	fn liquidity_delta_on_crossing_tick(tick_liquidity: &TickInfo) -> i128 {
		tick_liquidity.liquidity_delta
	}

	fn current_tick_after_crossing_target_tick(target_tick: Tick) -> Tick {
		target_tick
	}
}

#[derive(Debug)]
pub enum CreatePoolError {
	/// Fee must be between 0 - 50%
	InvalidFeeAmount,
	/// The initial price is outside the allowed range.
	InvalidInitialPrice,
}

#[derive(Debug)]
pub enum PositionError {
	/// Position referenced does not exist
	NonExistent,
	/// Position referenced does not contain the requested liquidity
	PositionLacksLiquidity,
}

#[derive(Debug)]
pub enum MintError<E> {
	/// Invalid Tick range
	InvalidTickRange,
	/// One of the start/end ticks of the range reached its maximum gross liquidity
	MaximumGrossLiquidity,
	/// The provided callback function didn't succeed.
	CallbackError(E),
}

#[derive(Debug, PartialEq, Eq)]
pub enum SwapError {
	InsufficientLiquidity,
}

#[derive(Debug)]
pub enum CollectError {}

#[derive(Clone, Debug, TypeInfo, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct PoolState {
	enabled: bool,
	fee_100th_bips: u32,
	current_sqrt_price: SqrtPriceQ64F96,
	current_tick: Tick,
	current_liquidity: Liquidity,
	global_fee_growth: PoolAssetMap<FeeGrowthQ128F128>,
	liquidity_map: BTreeMap<Tick, TickInfo>,
	positions: BTreeMap<(AccountId, Tick, Tick), Position>,
}

impl PoolState {
	/// Creates a new pool with the specified fee and initial price. The pool is created with no
	/// liquidity, it must be added using the `PoolState::mint` function.
	///
	/// This function will panic if fee_100th_bips or initial_sqrt_price are outside the allowed
	/// bounds
	pub fn new(
		fee_100th_bips: u32,
		initial_sqrt_price: SqrtPriceQ64F96,
	) -> Result<Self, CreatePoolError> {
		if fee_100th_bips > MAX_FEE_100TH_BIPS {
			return Err(CreatePoolError::InvalidFeeAmount)
		};
		if initial_sqrt_price < MIN_SQRT_PRICE || initial_sqrt_price >= MAX_SQRT_PRICE {
			return Err(CreatePoolError::InvalidInitialPrice)
		}
		let initial_tick = Self::tick_at_sqrt_price(initial_sqrt_price);
		Ok(Self {
			enabled: true,
			fee_100th_bips,
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
		})
	}

	/// Update the pool state to enable/disable the pool
	pub fn update_pool_enabled(&mut self, enabled: bool) {
		self.enabled = enabled;
	}

	/// Gets the current pool state (enabled/disabled)
	pub fn pool_enabled(&self) -> bool {
		self.enabled
	}

	/// Gets the current price of the pool in Tick
	pub fn current_tick(&self) -> Tick {
		self.current_tick
	}

	/// Tries to add `minted_liquidity` to/create the specified position, if `minted_liquidity == 0`
	/// no position will be created or have liquidity added, the callback will not be called, and
	/// the function will return `Ok(())`. Otherwise if the minting is possible the callback `f`
	/// will be passed the Amounts required to add the specified `minted_liquidity` to the specified
	/// position. The callback should return a boolean specifying if the liquidity minting should
	/// occur. If `false` is returned the position will not be created. If 'Ok(())' is returned the
	/// position will be created if it didn't already exist, and `minted_liquidity` will be added to
	/// it. Then the function will return `Ok(())`. Otherwise if the minting is not possible the
	/// callback will not be called, no state will be affected, and the function will return Err(_),
	/// with appropiate Error variant.
	///
	/// This function never panics
	///
	/// If this function returns an `Err(_)` no state changes have occurred
	pub fn mint<E>(
		&mut self,
		lp: AccountId,
		lower_tick: Tick,
		upper_tick: Tick,
		minted_liquidity: Liquidity,
		try_debit: impl FnOnce(PoolAssetMap<AmountU256>) -> Result<(), E>,
	) -> Result<(PoolAssetMap<AmountU256>, PoolAssetMap<u128>), MintError<E>> {
		if (lower_tick >= upper_tick) || (lower_tick < MIN_TICK) || (upper_tick > MAX_TICK) {
			return Err(MintError::InvalidTickRange)
		}

		if minted_liquidity == 0 {
			return Ok(Default::default())
		}

		let mut position = self
			.positions
			.get(&(lp.clone(), lower_tick, upper_tick))
			.cloned()
			.unwrap_or(
				Position{
					liquidity: 0,
					last_fee_growth_inside: Default::default(),
			});

		let tick_info_with_updated_gross_liquidity = |tick| {
			let mut tick_info = self.liquidity_map.get(&tick).cloned().unwrap_or_else(|| {
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

		lower_info.liquidity_delta = lower_info
			.liquidity_delta
			.checked_add_unsigned(minted_liquidity)
			.expect("Cannot overflow as liquidity_delta.abs() is bounded to <= MAX_TICK_GROSS_LIQUIDITY");
		let mut upper_info = tick_info_with_updated_gross_liquidity(upper_tick)?;
		
		upper_info.liquidity_delta = upper_info
			.liquidity_delta
			.checked_sub_unsigned(minted_liquidity)
			.expect("Cannot underflow as liquidity_delta.abs() is bounded to <= MAX_TICK_GROSS_LIQUIDITY");

		let fees_returned = position.set_liquidity(
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

		try_debit(amounts_required).map_err(MintError::CallbackError)?;
		self.current_liquidity += current_liquidity_delta;
		self.positions.insert((lp, lower_tick, upper_tick), position);
		self.liquidity_map.insert(lower_tick, lower_info);
		self.liquidity_map.insert(upper_tick, upper_info);

		Ok((amounts_required, fees_returned))
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
		lp: AccountId,
		lower_tick: Tick,
		upper_tick: Tick,
		burnt_liquidity: Liquidity,
	) -> Result<(PoolAssetMap<AmountU256>, PoolAssetMap<u128>), PositionError> {
		if let Some(mut position) =
			self.positions.get(&(lp.clone(), lower_tick, upper_tick)).cloned()
		{
			debug_assert!(position.liquidity != 0);
			if burnt_liquidity <= position.liquidity {
				let mut lower_info = *self.liquidity_map.get(&lower_tick).expect("lower_tick is guaranteed to exist.");
				let mut upper_info = *self.liquidity_map.get(&upper_tick).expect("upper_tick is guaranteed to exist.");

				if lower_info.liquidity_gross < burnt_liquidity ||
					upper_info.liquidity_gross < burnt_liquidity
				{
					return Err(PositionError::PositionLacksLiquidity)
				}

				lower_info.liquidity_gross =
					lower_info.liquidity_gross.saturating_sub(burnt_liquidity);
				lower_info.liquidity_delta = lower_info
					.liquidity_delta
					.checked_sub_unsigned(burnt_liquidity)
					.expect("Cannot underflow as liquidity_delta.abs() is bounded to <= MAX_TICK_GROSS_LIQUIDITY");

				upper_info.liquidity_gross =
					upper_info.liquidity_gross.saturating_sub(burnt_liquidity);
				upper_info.liquidity_delta = lower_info
					.liquidity_delta
					.checked_add_unsigned(burnt_liquidity)
					.expect("Cannot overflow as liquidity_delta.abs() is bounded to <= MAX_TICK_GROSS_LIQUIDITY");

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

				if lower_info.liquidity_gross == 0 && lower_tick != MIN_TICK
				// Guarantee MIN_TICK is always in map to simplify swap logic
				{
					debug_assert_eq!(position.liquidity, 0);
					self.liquidity_map.remove(&lower_tick);
				} else {
					self.liquidity_map.insert(lower_tick, lower_info);
				}
				if upper_info.liquidity_gross == 0 && upper_tick != MAX_TICK
				// Guarantee MAX_TICK is always in map to simplify swap logic
				{
					debug_assert_eq!(position.liquidity, 0);
					self.liquidity_map.remove(&upper_tick);
				} else {
					self.liquidity_map.insert(upper_tick, upper_info);
				}

				if position.liquidity == 0 {
					// DIFF: This behaviour is different than Uniswap's to ensure if a position
					// exists its ticks also exist in the liquidity_map
					// In other words, the position will automatically be removed if all the
					// liquidity has been burnt.
					self.positions.remove(&(lp, lower_tick, upper_tick));
				} else {
					// Reinsert the updated position back into storage.
					self.positions.insert((lp, lower_tick, upper_tick), position);
				}

				Ok((amounts_owed, fees_owed))
			} else {
				Err(PositionError::PositionLacksLiquidity)
			}
		} else {
			Err(PositionError::NonExistent)
		}
	}

	/// Returns liquidity held in a position.
	pub fn minted_liquidity(&self, lp: AccountId, range: AmmRange) -> Liquidity {
		match self.positions.get(&(lp, range.lower, range.upper)) {
			Some(position) => position.liquidity,
			None => Default::default(),
		}
	}

	pub fn set_liquidity_fees(&mut self, fee_100th_bips: u32) -> Result<(), CreatePoolError> {
		if fee_100th_bips > MAX_FEE_100TH_BIPS {
			Err(CreatePoolError::InvalidFeeAmount)
		} else {
			self.fee_100th_bips = fee_100th_bips;
			Ok(())
		}
	}

	pub fn get_liquidity_fees(&self) -> u32 {
		self.fee_100th_bips
	}

	/// Swaps the specified Amount of Asset 0 into Asset 1. Returns the Output and Fee amount.
	///
	/// This function never panics
	pub fn swap_from_asset_0_to_asset_1(
		&mut self,
		amount: AmountU256,
	) -> Result<(AmountU256, AmountU256), SwapError> {
		self.swap::<Asset0ToAsset1>(amount)
	}

	/// Swaps the specified Amount of Asset 1 into Asset 0. Returns the Output and Fee amount.
	///
	/// This function never panics
	pub fn swap_from_asset_1_to_asset_0(
		&mut self,
		amount: AmountU256,
	) -> Result<(AmountU256, AmountU256), SwapError> {
		self.swap::<Asset1ToAsset0>(amount)
	}

	/// Swaps the specified Amount into the other currency. Returns the Output and Fees amount. The
	/// direction of the swap is controlled by the generic type parameter `SD`, by setting it to
	/// `Asset0ToAsset1` or `Asset1ToAsset0`.
	///
	/// Returns Ok((output_amount, liquidity_fee)) or SwapError.
	fn swap<SD: SwapDirection>(
		&mut self,
		mut amount: AmountU256,
	) -> Result<(AmountU256, AmountU256), SwapError> {
		let mut total_amount_out = AmountU256::zero();
		let mut total_fee_paid = AmountU256::zero();

		while !amount.is_zero() {
			// Gets the next available liquidity, if there are any.
			if let Some((target_tick, target_info)) =
				SD::target_tick(self.current_tick, &mut self.liquidity_map)
			{
				let sqrt_ratio_target = Self::sqrt_price_at_tick(*target_tick);

				let amount_minus_fees = mul_div_floor(
					amount,
					U256::from(ONE_IN_HUNDREDTH_BIPS - self.fee_100th_bips),
					U256::from(ONE_IN_HUNDREDTH_BIPS),
				); // This cannot overflow as we bound fee_100th_bips to <= ONE_IN_HUNDREDTH_BIPS/2

				let amount_required_to_reach_target = SD::input_amount_delta_ceil(
					self.current_sqrt_price,
					sqrt_ratio_target,
					self.current_liquidity,
				);

				let sqrt_ratio_next = if amount_minus_fees >= amount_required_to_reach_target {
					sqrt_ratio_target
				} else {
					debug_assert!(self.current_liquidity != 0);
					SD::next_sqrt_price_from_input_amount(
						self.current_sqrt_price,
						self.current_liquidity,
						amount_minus_fees,
					)
				};

				// Cannot overflow as if the swap traversed all ticks (MIN_TICK to MAX_TICK
				// (inclusive)), assuming the maximum possible liquidity, total_amount_out would
				// still be below U256::MAX (See test `output_amounts_bounded`)
				total_amount_out = total_amount_out.saturating_add(SD::output_amount_delta_floor(
					self.current_sqrt_price,
					sqrt_ratio_next,
					self.current_liquidity,
				));

				// next_sqrt_price_from_input_amount rounds so this maybe Ok(()) even though
				// amount_minus_fees < amount_required_to_reach_target (TODO Prove)
				if sqrt_ratio_next == sqrt_ratio_target {
					let fees = mul_div_ceil(
						amount_required_to_reach_target,
						U256::from(self.fee_100th_bips),
						// Will not overflow as fee_100th_bips <= ONE_IN_HUNDREDTH_BIPS / 2
						U256::from(ONE_IN_HUNDREDTH_BIPS.saturating_sub(self.fee_100th_bips)),
					);

					// DIFF: This behaviour is different to Uniswap's, we saturate instead of
					// overflowing/bricking the pool. This means we just stop giving LPs fees, but
					// this is exceptionally unlikely to occur due to the how large the maximum
					// global_fee_growth value is. We also do this to avoid needing to consider the
					// case of reverting an extrinsic's mutations which is expensive in Substrate
					// based chains.
					if self.current_liquidity > 0 {
						self.global_fee_growth[SD::INPUT_SIDE] =
							self.global_fee_growth[SD::INPUT_SIDE].saturating_add(mul_div_floor(
								fees,
								U256::from(1) << 128u32,
								self.current_liquidity,
							));
						target_info.fee_growth_outside = PoolAssetMap::new_from_fn(|side| {
							self.global_fee_growth[side]
								.saturating_sub(target_info.fee_growth_outside[side])
						});
					}

					self.current_sqrt_price = sqrt_ratio_target;
					self.current_tick = SD::current_tick_after_crossing_target_tick(*target_tick);

					// TODO: Prove these don't underflow
					amount = amount.saturating_sub(amount_required_to_reach_target);
					amount = amount.saturating_sub(fees);

					total_fee_paid = total_fee_paid.saturating_add(fees);

					let liquidity_delta = SD::liquidity_delta_on_crossing_tick(target_info);
					self.current_liquidity = self.current_liquidity.checked_add_signed(liquidity_delta).expect("Addition is guaranteed to never overflow, see test `max_liquidity`");
				} else {
					let amount_in = SD::input_amount_delta_ceil(
						self.current_sqrt_price,
						sqrt_ratio_next,
						self.current_liquidity,
					);
					// Will not underflow due to rounding in flavor of the pool of both
					// sqrt_ratio_next and amount_in. (TODO: Prove)
					let fees = amount.saturating_sub(amount_in);
					total_fee_paid = total_fee_paid.saturating_add(fees);

					// DIFF: This behaviour is different to Uniswap's,
					// we saturate instead of overflowing/bricking the pool. This means we just stop
					// giving LPs fees, but this is exceptionally unlikely to occur due to the how
					// large the maximum global_fee_growth value is. We also do this to avoid
					// needing to consider the case of reverting an extrinsic's mutations which is
					// expensive in Substrate based chains.
					if self.current_liquidity > 0 {
						self.global_fee_growth[SD::INPUT_SIDE] =
							self.global_fee_growth[SD::INPUT_SIDE].saturating_add(mul_div_floor(
								fees,
								U256::from(1) << 128u32,
								self.current_liquidity,
							));
					}
					// Recompute unless we're on a lower tick boundary (i.e. already
					// transitioned ticks), and haven't moved (test_updates_exiting &
					// test_updates_entering)
					if self.current_sqrt_price != sqrt_ratio_next {
						self.current_sqrt_price = sqrt_ratio_next;
						self.current_tick = Self::tick_at_sqrt_price(self.current_sqrt_price);
					}

					break
				};
			} else {
				// There are not enough liquidity left in the pool to complete the swap.
				return Err(SwapError::InsufficientLiquidity)
			}
		}

		Ok((total_amount_out, total_fee_paid))
	}

	fn liquidity_to_amounts<const ROUND_UP: bool>(
		&self,
		liquidity: Liquidity,
		lower_tick: Tick,
		upper_tick: Tick,
	) -> (PoolAssetMap<AmountU256>, Liquidity) {
		if self.current_tick < lower_tick {
			(
				PoolAssetMap::new(
					(if ROUND_UP {
						Self::asset_0_amount_delta_ceil
					} else {
						Self::asset_0_amount_delta_floor
					})(
						Self::sqrt_price_at_tick(lower_tick),
						Self::sqrt_price_at_tick(upper_tick),
						liquidity,
					),
					0.into(),
				),
				0,
			)
		} else if self.current_tick < upper_tick {
			(
				PoolAssetMap::new(
					(if ROUND_UP {
						Self::asset_0_amount_delta_ceil
					} else {
						Self::asset_0_amount_delta_floor
					})(self.current_sqrt_price, Self::sqrt_price_at_tick(upper_tick), liquidity),
					(if ROUND_UP {
						Self::asset_1_amount_delta_ceil
					} else {
						Self::asset_1_amount_delta_floor
					})(Self::sqrt_price_at_tick(lower_tick), self.current_sqrt_price, liquidity),
				),
				liquidity,
			)
		} else {
			(
				PoolAssetMap::new(
					0.into(),
					(if ROUND_UP {
						Self::asset_1_amount_delta_ceil
					} else {
						Self::asset_1_amount_delta_floor
					})(
						Self::sqrt_price_at_tick(lower_tick),
						Self::sqrt_price_at_tick(upper_tick),
						liquidity,
					),
				),
				0,
			)
		}
	}

	fn asset_0_amount_delta_floor(
		from: SqrtPriceQ64F96,
		to: SqrtPriceQ64F96,
		liquidity: Liquidity,
	) -> AmountU256 {
		debug_assert!(SqrtPriceQ64F96::zero() < from);
		debug_assert!(from <= to);

		/*
			Proof that `mul_div` does not overflow:
			If A ∈ ℕ, B ∈ ℕ, A > 0, B >= A
			Then A * B >= B and B - A < B
			Then A * B > B - A
		*/
		mul_div_floor(
			U256::from(liquidity) << 96u32,
			to.saturating_sub(from),
			U256::full_mul(to, from),
		)
	}

	fn asset_0_amount_delta_ceil(
		from: SqrtPriceQ64F96,
		to: SqrtPriceQ64F96,
		liquidity: Liquidity,
	) -> AmountU256 {
		debug_assert!(SqrtPriceQ64F96::zero() < from);
		debug_assert!(from <= to);

		/*
			Proof that `mul_div` does not overflow:
			If A ∈ ℕ, B ∈ ℕ, A > 0, B >= A
			Then A * B >= B and B - A < B
			Then A * B > B - A
		*/
		mul_div_ceil(
			U256::from(liquidity) << 96u32,
			to.saturating_sub(from),
			U256::full_mul(to, from),
		)
	}

	fn asset_1_amount_delta_floor(
		from: SqrtPriceQ64F96,
		to: SqrtPriceQ64F96,
		liquidity: Liquidity,
	) -> AmountU256 {
		debug_assert!(SqrtPriceQ64F96::zero() < from);
		// NOTE: When minting/burning at lowertick == currenttick, from == to. When swapping only
		// from < to. To refine the check?
		debug_assert!(from <= to);

		/*
			Proof that `mul_div` does not overflow:
			If A ∈ u160, B ∈ u160, A < B, L ∈ u128
			Then B - A ∈ u160
			Then (B - A) / (1<<96) <= u64::MAX (160 - 96 = 64)
			Then L * ((B - A) / (1<<96)) <= u192::MAX < u256::MAX
		*/
		mul_div_floor(liquidity.into(), to.saturating_sub(from), U512::from(1) << 96u32)
	}

	fn asset_1_amount_delta_ceil(
		from: SqrtPriceQ64F96,
		to: SqrtPriceQ64F96,
		liquidity: Liquidity,
	) -> AmountU256 {
		debug_assert!(SqrtPriceQ64F96::zero() < from);
		// NOTE: When minting/burning at lowertick == currenttick, from == to. When swapping only
		// from < to. To refine the check?
		debug_assert!(from <= to);

		/*
			Proof that `mul_div` does not overflow:
			If A ∈ u160, B ∈ u160, A < B, L ∈ u128
			Then B - A ∈ u160
			Then (B - A) / (1<<96) <= u64::MAX (160 - 96 = 64)
			Then L * ((B - A) / (1<<96)) <= u192::MAX < u256::MAX
		*/
		mul_div_ceil(liquidity.into(), to.saturating_sub(from), U512::from(1u32) << 96u32)
	}

	fn next_sqrt_price_from_asset_0_input(
		sqrt_ratio_current: SqrtPriceQ64F96,
		liquidity: Liquidity,
		amount: AmountU256,
	) -> SqrtPriceQ64F96 {
		debug_assert!(0 < liquidity);
		debug_assert!(SqrtPriceQ64F96::zero() < sqrt_ratio_current);

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
			U512::from(liquidity).saturating_add(U256::full_mul(amount, sqrt_ratio_current)),
		)
	}

	fn next_sqrt_price_from_asset_1_input(
		sqrt_ratio_current: SqrtPriceQ64F96,
		liquidity: Liquidity,
		amount: AmountU256,
	) -> SqrtPriceQ64F96 {
		debug_assert!(liquidity > 0);
		// Will not overflow as function is not called if amount >= amount_required_to_reach_target,
		// therefore bounding the function output to approximately <= MAX_SQRT_PRICE
		sqrt_ratio_current.saturating_add(
			(amount << 96u32)
				.checked_div(liquidity.into())
				.expect("Liquidity should never be zero"),
		)
	}

	pub fn sqrt_price_at_tick(tick: Tick) -> SqrtPriceQ64F96 {
		debug_assert!((MIN_TICK..=MAX_TICK).contains(&tick));

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
				U256::saturating_mul(U256::one() << 128u128, $constant.into());
				if abs_tick & (0x1u32 << $bit) != 0 {
					r = U256::saturating_mul(r, U256::from($constant)) >> 128u128
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
		// r is guaranteed to be > 0
		debug_assert!(!r.is_zero());
		let sqrt_price_q32f128 = if tick > 0 { U256::MAX / r } else { r };

		// we round up in the division so tick_at_sqrt_price of the output price is always
		// consistent
		(sqrt_price_q32f128 >> 32u128).saturating_add(if sqrt_price_q32f128.low_u32() == 0 {
			U256::zero()
		} else {
			U256::one()
		})
	}

	/// Calculates the greatest tick value such that `sqrt_price_at_tick(tick) <= sqrt_price`
	pub fn tick_at_sqrt_price(sqrt_price: SqrtPriceQ64F96) -> Tick {
		debug_assert!(sqrt_price >= MIN_SQRT_PRICE);
		// Note the price can never actually reach MAX_SQRT_PRICE
		debug_assert!(sqrt_price < MAX_SQRT_PRICE);

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
				.try_into()
				.expect("Conversion to u128 is safe as top 128 bits are always zero"),
			)
		};

		let log_2_q63f64 = {
			let mut log_2_q63f64 = (integer_log_2 as i128) << 64u8;
			let mut _mantissa: u128 = mantissa;

			// rustfmt chokes when formatting this macro.
			// See: https://github.com/rust-lang/rustfmt/issues/5404
			#[rustfmt::skip]
			macro_rules! add_fractional_bit {
				($bit:literal) => {
					// Note squaring a number doubles its log
					let mantissa_sq =
						(U256::saturating_mul(_mantissa.into(), _mantissa.into()) >> 127u8);
					_mantissa = if mantissa_sq.bit(128) {
						// is the 129th bit set, all higher bits must be zero due to 127 right bit
						// shift
						log_2_q63f64 |= (1i128 << $bit);
						(mantissa_sq >> 1u8).try_into().expect("Conversion to u128 is safe: top 128 bits are always zero")
					} else {
						mantissa_sq.try_into().expect("Conversion to u128 is safe: top 128 bits are always zero")
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

		let tick_low: Tick = (U256::overflowing_sub(
			log_sqrt10001_q127f128,
			U256::from(3402992956809132418596140100660247210u128),
		)
		.0 >> 128u8)
			.try_into().expect("Right shifts ensures the top bits are 0");
		let tick_high: Tick = (U256::overflowing_add(
			log_sqrt10001_q127f128,
			U256::from(291339464771989622907027621153398088495u128),
		)
		.0 >> 128u8).try_into().expect("Right shifts ensures the top bits are 0");

		if tick_low == tick_high {
			tick_low
		} else if Self::sqrt_price_at_tick(tick_high) <= sqrt_price {
			tick_high
		} else {
			tick_low
		}
	}
}

fn mul_div_floor<C: Into<U512>>(a: U256, b: U256, c: C) -> U256 {
	let c: U512 = c.into();
	debug_assert!(!c.is_zero());
	(U256::full_mul(a, b) / c)
		.try_into()
		.expect("Core AMM arithmetic should never divide by zero, and cast to U256 must succeed")
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
	.expect("Core AMM arithmetic should never divide by zero, and cast to U256 must succeed")
}
