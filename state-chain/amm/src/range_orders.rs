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
//!
//! Some of the `proofs` assume the values of SqrtPriceQ64F96 are <= U160::MAX, but as that type
//! doesn't exist we use U256 of SqrtPriceQ64F96. It is relatively simply to verify that all
//! instances of SqrtPriceQ64F96 are <=U160::MAX.

mod tests;

use sp_std::{collections::btree_map::BTreeMap, convert::Infallible};

use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_core::{U256, U512};

use crate::common::{
	is_sqrt_price_valid, mul_div_ceil, mul_div_floor, sqrt_price_at_tick, tick_at_sqrt_price,
	Amount, OneToZero, Side, SideMap, SqrtPriceQ64F96, Tick, ZeroToOne, MAX_TICK, MIN_TICK,
	ONE_IN_HUNDREDTH_PIPS, SQRT_PRICE_FRACTIONAL_BITS,
};

pub type Liquidity = u128;
type FeeGrowthQ128F128 = U256;

const MAX_TICK_GROSS_LIQUIDITY: Liquidity = Liquidity::MAX / ((1 + MAX_TICK - MIN_TICK) as u128);

#[derive(Clone, Debug, TypeInfo, Encode, Decode, MaxEncodedLen)]
#[cfg_attr(feature = "std", derive(Deserialize, Serialize))]
pub struct Position {
	liquidity: Liquidity,
	#[cfg_attr(feature = "std", serde(skip))]
	last_fee_growth_inside: SideMap<FeeGrowthQ128F128>,
}

impl Position {
	fn collect_fees<LiquidityProvider>(
		&mut self,
		pool_state: &PoolState<LiquidityProvider>,
		lower_tick: Tick,
		lower_delta: &TickDelta,
		upper_tick: Tick,
		upper_delta: &TickDelta,
	) -> Collected {
		let fee_growth_inside = SideMap::default().map(|side, ()| {
			let fee_growth_below = if pool_state.current_tick < lower_tick {
				pool_state.global_fee_growth[side] - lower_delta.fee_growth_outside[side]
			} else {
				lower_delta.fee_growth_outside[side]
			};

			let fee_growth_above = if pool_state.current_tick < upper_tick {
				upper_delta.fee_growth_outside[side]
			} else {
				pool_state.global_fee_growth[side] - upper_delta.fee_growth_outside[side]
			};

			pool_state.global_fee_growth[side] - fee_growth_below - fee_growth_above
		});
		let collected_fees = Collected {
			fees: SideMap::default().map(|side, ()| {
				// DIFF: This behaviour is different than Uniswap's. We use U256 instead of u128 to
				// calculate fees, therefore it is not possible to overflow the fees here.

				/*
					Proof that `mul_div_floor` does not overflow:
					Note position.liqiudity: u128
					U512::one() << 128 > u128::MAX
				*/
				mul_div_floor(
					fee_growth_inside[side] - self.last_fee_growth_inside[side],
					self.liquidity.into(),
					U512::one() << 128,
				)
			}),
		};
		self.last_fee_growth_inside = fee_growth_inside;
		collected_fees
	}

	fn set_liquidity<LiquidityProvider>(
		&mut self,
		pool_state: &PoolState<LiquidityProvider>,
		new_liquidity: Liquidity,
		lower_tick: Tick,
		lower_delta: &TickDelta,
		upper_tick: Tick,
		upper_delta: &TickDelta,
	) -> (Collected, PositionInfo) {
		let collected_fees =
			self.collect_fees(pool_state, lower_tick, lower_delta, upper_tick, upper_delta);
		self.liquidity = new_liquidity;
		(collected_fees, PositionInfo::from(&*self))
	}
}

#[derive(Clone, Debug, TypeInfo, Encode, Decode, MaxEncodedLen)]
#[cfg_attr(feature = "std", derive(Deserialize, Serialize))]
pub struct TickDelta {
	liquidity_delta: i128,
	liquidity_gross: u128,
	fee_growth_outside: SideMap<FeeGrowthQ128F128>,
}

#[derive(Clone, Debug, TypeInfo, Encode, Decode)]
#[cfg_attr(feature = "std", derive(Deserialize, Serialize))]
#[cfg_attr(
	feature = "std",
	serde(bound = "LiquidityProvider: Ord + Serialize + serde::de::DeserializeOwned")
)]
pub struct PoolState<LiquidityProvider> {
	fee_hundredth_pips: u32,
	// Note the current_sqrt_price can reach MAX_SQRT_PRICE, but only if the tick is MAX_TICK
	current_sqrt_price: SqrtPriceQ64F96,
	current_tick: Tick,
	current_liquidity: Liquidity,
	#[cfg_attr(feature = "std", serde(skip))]
	global_fee_growth: SideMap<FeeGrowthQ128F128>,
	#[cfg_attr(feature = "std", serde(skip))]
	liquidity_map: BTreeMap<Tick, TickDelta>,
	#[cfg_attr(feature = "std", serde(with = "cf_utilities::serde_helpers::map_as_seq"))]
	positions: BTreeMap<(LiquidityProvider, Tick, Tick), Position>,
}

pub trait SwapDirection: crate::common::SwapDirection {
	/// Given the current_tick determines if the current price can increase further i.e. that there
	/// is possibly liquidity past the current price
	fn further_liquidity(current_tick: Tick) -> bool;

	/// The xy=k maths only works while the liquidity is constant, so this function returns the
	/// closest (to the current) next tick/price where liquidity possibly changes. Note the
	/// direction of `next` is implied by the swapping direction.
	fn next_liquidity_delta(
		current_tick: Tick,
		liquidity_map: &mut BTreeMap<Tick, TickDelta>,
	) -> Option<(&Tick, &mut TickDelta)>;

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
		sqrt_price_current: SqrtPriceQ64F96,
		liquidity: Liquidity,
		amount: Amount,
	) -> SqrtPriceQ64F96;

	/// For a given tick calculates the change in current liquidity when that tick is crossed
	fn liquidity_delta_on_crossing_tick(tick_liquidity: &TickDelta) -> i128;

	/// The current tick is always the closest tick less than the current_sqrt_price
	fn current_tick_after_crossing_tick(tick: Tick) -> Tick;
}

impl SwapDirection for ZeroToOne {
	fn further_liquidity(current_tick: Tick) -> bool {
		current_tick >= MIN_TICK
	}

	fn next_liquidity_delta(
		current_tick: Tick,
		liquidity_map: &mut BTreeMap<Tick, TickDelta>,
	) -> Option<(&Tick, &mut TickDelta)> {
		assert!(liquidity_map.contains_key(&MIN_TICK));
		if Self::further_liquidity(current_tick) {
			Some(liquidity_map.range_mut(..=current_tick).next_back().unwrap())
		} else {
			assert_eq!(current_tick, Self::current_tick_after_crossing_tick(MIN_TICK));
			None
		}
	}

	fn input_amount_delta_ceil(
		current: SqrtPriceQ64F96,
		target: SqrtPriceQ64F96,
		liquidity: Liquidity,
	) -> Amount {
		zero_amount_delta_ceil(target, current, liquidity)
	}

	fn output_amount_delta_floor(
		current: SqrtPriceQ64F96,
		target: SqrtPriceQ64F96,
		liquidity: Liquidity,
	) -> Amount {
		one_amount_delta_floor(target, current, liquidity)
	}

	fn next_sqrt_price_from_input_amount(
		sqrt_price_current: SqrtPriceQ64F96,
		liquidity: Liquidity,
		amount: Amount,
	) -> SqrtPriceQ64F96 {
		assert!(0 < liquidity);
		assert!(SqrtPriceQ64F96::zero() < sqrt_price_current);

		let liquidity = U256::from(liquidity) << SQRT_PRICE_FRACTIONAL_BITS;

		/*
			Proof that `mul_div_ceil` does not overflow:
			If L ∈ u256, R ∈ u256, A ∈ u256
			Then L <= L + R * A
			Then L / (L + R * A) <= 1
			Then R * L / (L + R * A) <= u256::MAX
		*/
		mul_div_ceil(
			liquidity,
			sqrt_price_current,
			// Addition will not overflow as function is not called if amount >=
			// amount_required_to_reach_target
			U512::from(liquidity) + U256::full_mul(amount, sqrt_price_current),
		)
	}

	fn liquidity_delta_on_crossing_tick(tick_liquidity: &TickDelta) -> i128 {
		-tick_liquidity.liquidity_delta
	}

	fn current_tick_after_crossing_tick(tick: Tick) -> Tick {
		tick - 1
	}
}

impl SwapDirection for OneToZero {
	fn further_liquidity(current_tick: Tick) -> bool {
		current_tick < MAX_TICK
	}

	fn next_liquidity_delta(
		current_tick: Tick,
		liquidity_map: &mut BTreeMap<Tick, TickDelta>,
	) -> Option<(&Tick, &mut TickDelta)> {
		assert!(liquidity_map.contains_key(&MAX_TICK));
		if Self::further_liquidity(current_tick) {
			Some(liquidity_map.range_mut(current_tick + 1..).next().unwrap())
		} else {
			assert_eq!(current_tick, Self::current_tick_after_crossing_tick(MAX_TICK));
			None
		}
	}

	fn input_amount_delta_ceil(
		current: SqrtPriceQ64F96,
		target: SqrtPriceQ64F96,
		liquidity: Liquidity,
	) -> Amount {
		one_amount_delta_ceil(current, target, liquidity)
	}

	fn output_amount_delta_floor(
		current: SqrtPriceQ64F96,
		target: SqrtPriceQ64F96,
		liquidity: Liquidity,
	) -> Amount {
		zero_amount_delta_floor(current, target, liquidity)
	}

	fn next_sqrt_price_from_input_amount(
		sqrt_price_current: SqrtPriceQ64F96,
		liquidity: Liquidity,
		amount: Amount,
	) -> SqrtPriceQ64F96 {
		assert!(0 < liquidity);

		// Will not overflow as function is not called if amount >= amount_required_to_reach_target,
		// therefore bounding the function output to approximately <= MAX_SQRT_PRICE
		sqrt_price_current +
			mul_div_floor(amount, U256::one() << SQRT_PRICE_FRACTIONAL_BITS, liquidity)
	}

	fn liquidity_delta_on_crossing_tick(tick_liquidity: &TickDelta) -> i128 {
		tick_liquidity.liquidity_delta
	}

	fn current_tick_after_crossing_tick(tick: Tick) -> Tick {
		tick
	}
}

#[derive(Debug)]
pub enum NewError {
	/// Fee must be between 0 - 50%
	InvalidFeeAmount,
	/// The initial price is outside the allowed range.
	InvalidInitialPrice,
}

#[derive(Debug)]
pub enum SetFeesError {
	/// Fee must be between 0 - 50%
	InvalidFeeAmount,
}

#[derive(Debug)]
pub enum MintError<E> {
	/// One of the start/end ticks of the range reached its maximum gross liquidity
	MaximumGrossLiquidity,
	/// The ratio of assets added to the position must match the required ratio of assets for the
	/// given tick range and current price of the pool, but there are no amounts between the
	/// specified maximum and minimum that could match that ratio
	AssetRatioUnachieveable,
	/// Callback failed
	CallbackFailed(E),
}

#[derive(Debug)]
pub enum PositionError<T> {
	/// Invalid Tick range
	InvalidTickRange,
	/// Position referenced does not exist
	NonExistent,
	Other(T),
}

#[derive(Debug)]
pub enum BurnError {
	/// The ratio of assets removed from the position must match the ratio of assets in the
	/// position, so that the ratio of assets in the position is maintained, but there are no
	/// amounts between the specified maximum and minimum that could match that ratio
	AssetRatioUnachieveable,
}

#[derive(Debug)]
pub enum CollectError {}

#[derive(Debug)]
pub enum LiquidityToAmountsError {
	/// Invalid Tick range
	InvalidTickRange,
	/// The specified liquidity is greater than the maximum
	InvalidLiquidityAmount,
}

#[derive(Debug)]
pub enum AmountsToLiquidityError {
	/// Invalid Tick range
	InvalidTickRange,
}

#[derive(Default, Debug, PartialEq, Eq, TypeInfo, Encode, Decode, MaxEncodedLen)]
pub struct Collected {
	pub fees: SideMap<Amount>,
}

#[derive(Debug, PartialEq, Eq, TypeInfo, Encode, Decode, MaxEncodedLen)]
pub enum Size {
	Liquidity { liquidity: Liquidity },
	Amount { maximum: SideMap<Amount>, minimum: SideMap<Amount> },
}

#[derive(Default, Debug, PartialEq, Eq, TypeInfo, Encode, Decode, MaxEncodedLen)]
pub struct PositionInfo {
	pub liquidity: Liquidity,
}
impl PositionInfo {
	pub fn new(liquidity: Liquidity) -> Self {
		Self { liquidity }
	}
}
impl<'a> From<&'a Position> for PositionInfo {
	fn from(value: &'a Position) -> Self {
		Self { liquidity: value.liquidity }
	}
}

impl<LiquidityProvider: Clone + Ord> PoolState<LiquidityProvider> {
	/// Creates a new pool with the specified fee and initial price. The pool is created with no
	/// liquidity, it must be added using the `PoolState::collect_and_mint` function.
	///
	/// This function never panics
	pub fn new(fee_hundredth_pips: u32, initial_sqrt_price: U256) -> Result<Self, NewError> {
		Self::validate_fees(fee_hundredth_pips)
			.then_some(())
			.ok_or(NewError::InvalidFeeAmount)?;
		is_sqrt_price_valid(initial_sqrt_price)
			.then_some(())
			.ok_or(NewError::InvalidInitialPrice)?;
		let initial_sqrt_price: SqrtPriceQ64F96 = initial_sqrt_price;

		let initial_tick = tick_at_sqrt_price(initial_sqrt_price);
		Ok(Self {
			fee_hundredth_pips,
			current_sqrt_price: initial_sqrt_price,
			current_tick: initial_tick,
			current_liquidity: 0,
			global_fee_growth: Default::default(),
			// Guarantee MIN_TICK and MAX_TICK are always in map to simplify swap logic
			liquidity_map: [
				(
					MIN_TICK,
					TickDelta {
						liquidity_delta: 0,
						liquidity_gross: 0,
						fee_growth_outside: Default::default(),
					},
				),
				(
					MAX_TICK,
					TickDelta {
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

	/// Sets the fee for the pool. This will apply to future swaps. This function will fail if the
	/// fee is greater than 50%.
	///
	/// This function never panics
	pub fn set_fees(&mut self, fee_hundredth_pips: u32) -> Result<(), SetFeesError> {
		Self::validate_fees(fee_hundredth_pips)
			.then_some(())
			.ok_or(SetFeesError::InvalidFeeAmount)?;
		self.fee_hundredth_pips = fee_hundredth_pips;
		Ok(())
	}

	fn validate_fees(fee_hundredth_pips: u32) -> bool {
		fee_hundredth_pips <= ONE_IN_HUNDREDTH_PIPS / 2
	}

	/// Returns the current sqrt price of the pool. None if the pool has no more liquidity and the
	/// price cannot get worse.
	///
	/// This function never panics
	pub fn current_sqrt_price<SD: SwapDirection>(&self) -> Option<SqrtPriceQ64F96> {
		SD::further_liquidity(self.current_tick).then_some(self.current_sqrt_price)
	}

	/// Calculates the fees owed to the specified position, resets the fees owed for that position
	/// to zero, calls `try_debit` passing the Amounts required to add the `minted_liquidity` to the
	/// position. If `try_debit` returns `Ok(t)` the position will be created if it didn't already
	/// exist, `minted_liquidity` will be added to it, and `Ok((t, collected_fees))` will be
	/// returned. If `Err(_)` is returned the position will not be created, and `Err(_)`will be
	/// returned. If the minting would result in either the lower or upper tick having more
	/// liquidity than `MAX_TICK_GROSS_LIQUIDITY` associated with it, this function will return
	/// `Err(MintError::MaximumGrossLiquidity)`.
	///
	/// This function never panics
	///
	/// If this function returns an `Err(_)` no state changes have occurred
	pub fn collect_and_mint<T, E, TryDebit: FnOnce(SideMap<Amount>) -> Result<T, E>>(
		&mut self,
		lp: &LiquidityProvider,
		lower_tick: Tick,
		upper_tick: Tick,
		size: Size,
		try_debit: TryDebit,
	) -> Result<(T, Collected, PositionInfo), PositionError<MintError<E>>> {
		Self::validate_position_range(lower_tick, upper_tick)?;
		let option_position = self.positions.get(&(lp.clone(), lower_tick, upper_tick));

		let [option_initial_lower_delta, option_initial_upper_delta] =
			[lower_tick, upper_tick].map(|tick| self.liquidity_map.get(&tick));

		let minted_liquidity = self
			.size_as_liquidity(lower_tick, upper_tick, size)
			.ok_or(PositionError::Other(MintError::AssetRatioUnachieveable))
			.and_then(|liquidity| {
				if liquidity >
					MAX_TICK_GROSS_LIQUIDITY -
						[option_initial_lower_delta, option_initial_upper_delta]
							.into_iter()
							.filter_map(|option_tick_delta| {
								option_tick_delta.map(|tick_delta| tick_delta.liquidity_gross)
							})
							.max()
							.unwrap_or(0)
				{
					Err(PositionError::Other(MintError::MaximumGrossLiquidity))
				} else {
					Ok(liquidity)
				}
			})?;

		if option_position.is_some() || minted_liquidity != 0 {
			let mut position = option_position.cloned().unwrap_or_else(|| Position {
				liquidity: 0,
				last_fee_growth_inside: Default::default(),
			});

			let tick_delta_with_updated_gross_liquidity =
				|tick, option_initial_tick_delta: Option<&TickDelta>| {
					let mut tick_delta = option_initial_tick_delta.cloned().unwrap_or_else(|| {
						TickDelta {
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

					tick_delta.liquidity_gross += minted_liquidity;
					tick_delta
				};

			let mut lower_delta =
				tick_delta_with_updated_gross_liquidity(lower_tick, option_initial_lower_delta);
			// Cannot overflow as due to liquidity_gross's MAX_TICK_GROSS_LIQUIDITY bound
			lower_delta.liquidity_delta =
				lower_delta.liquidity_delta.checked_add_unsigned(minted_liquidity).unwrap();
			let mut upper_delta =
				tick_delta_with_updated_gross_liquidity(upper_tick, option_initial_upper_delta);
			// Cannot overflow as due to liquidity_gross's MAX_TICK_GROSS_LIQUIDITY bound
			upper_delta.liquidity_delta =
				upper_delta.liquidity_delta.checked_sub_unsigned(minted_liquidity).unwrap();

			let (collected_fees, position_info) = position.set_liquidity(
				self,
				// Cannot overflow due to MAX_TICK_GROSS_LIQUIDITY
				position.liquidity + minted_liquidity,
				lower_tick,
				&lower_delta,
				upper_tick,
				&upper_delta,
			);

			let (amounts_required, current_liquidity_delta) =
				self.inner_liquidity_to_amounts::<true>(minted_liquidity, lower_tick, upper_tick);

			let t = try_debit(amounts_required)
				.map_err(|err| PositionError::Other(MintError::CallbackFailed(err)))?;

			self.current_liquidity += current_liquidity_delta;
			self.positions.insert((lp.clone(), lower_tick, upper_tick), position);
			self.liquidity_map.insert(lower_tick, lower_delta);
			self.liquidity_map.insert(upper_tick, upper_delta);

			Ok((t, collected_fees, position_info))
		} else {
			Err(PositionError::NonExistent)
		}
	}

	/// Calculates the fees owed to the specified position, resets the fees owed for that
	/// position to zero, removes liquidity from the specified range-order, and returns the value of
	/// the burnt liquidity in `Amounts` with the calculated amount of owed fees. If all the
	/// position's liquidity is burned then it is destroyed. If the position does not exist returns
	/// `Err(_)`
	///
	/// This function never panics
	///
	/// If this function returns an `Err(_)` no state changes have occurred
	#[allow(clippy::type_complexity)]
	pub fn collect_and_burn(
		&mut self,
		lp: &LiquidityProvider,
		lower_tick: Tick,
		upper_tick: Tick,
		size: Size,
	) -> Result<(SideMap<Amount>, Collected, PositionInfo), PositionError<BurnError>> {
		Self::validate_position_range(lower_tick, upper_tick)?;
		if let Some(mut position) =
			self.positions.get(&(lp.clone(), lower_tick, upper_tick)).cloned()
		{
			assert!(position.liquidity != 0);

			let burnt_liquidity = self
				.size_as_liquidity(lower_tick, upper_tick, size)
				.ok_or(PositionError::Other(BurnError::AssetRatioUnachieveable))
				.map(|liquidity| core::cmp::min(position.liquidity, liquidity))?;

			let mut lower_delta = self.liquidity_map.get(&lower_tick).unwrap().clone();
			lower_delta.liquidity_gross -= burnt_liquidity;
			lower_delta.liquidity_delta =
				lower_delta.liquidity_delta.checked_sub_unsigned(burnt_liquidity).unwrap();
			let mut upper_delta = self.liquidity_map.get(&upper_tick).unwrap().clone();
			upper_delta.liquidity_gross -= burnt_liquidity;
			upper_delta.liquidity_delta =
				upper_delta.liquidity_delta.checked_add_unsigned(burnt_liquidity).unwrap();

			let (collected_fees, position_info) = position.set_liquidity(
				self,
				position.liquidity - burnt_liquidity,
				lower_tick,
				&lower_delta,
				upper_tick,
				&upper_delta,
			);

			let (amounts_owed, current_liquidity_delta) =
				self.inner_liquidity_to_amounts::<false>(burnt_liquidity, lower_tick, upper_tick);
			// Will not underflow as current_liquidity_delta must have previously been added to
			// current_liquidity for it to need to be substrated now
			self.current_liquidity -= current_liquidity_delta;

			if lower_delta.liquidity_gross == 0 &&
				/*Guarantee MIN_TICK is always in map to simplify swap logic*/ lower_tick != MIN_TICK
			{
				assert_eq!(position.liquidity, 0);
				self.liquidity_map.remove(&lower_tick);
			} else {
				*self.liquidity_map.get_mut(&lower_tick).unwrap() = lower_delta;
			}
			if upper_delta.liquidity_gross == 0 &&
				/*Guarantee MAX_TICK is always in map to simplify swap logic*/ upper_tick != MAX_TICK
			{
				assert_eq!(position.liquidity, 0);
				self.liquidity_map.remove(&upper_tick);
			} else {
				*self.liquidity_map.get_mut(&upper_tick).unwrap() = upper_delta;
			}

			if position.liquidity == 0 {
				// DIFF: This behaviour is different than Uniswap's to ensure if a position
				// exists its ticks also exist in the liquidity_map, by removing zero liquidity
				// positions
				self.positions.remove(&(lp.clone(), lower_tick, upper_tick));
			} else {
				*self.positions.get_mut(&(lp.clone(), lower_tick, upper_tick)).unwrap() = position;
			};

			// DIFF: This behaviour is different than Uniswap's. We don't accumulated tokens
			// owed in the position, instead it is returned here.
			Ok((amounts_owed, collected_fees, position_info))
		} else {
			Err(PositionError::NonExistent)
		}
	}

	/// Calculates the fees owed to the specified position, resets the fees owed for that
	/// position to zero, and returns the calculated amount of owed fees
	///
	/// This function never panics
	///
	/// If this function returns an `Err(_)` no state changes have occurred
	pub fn collect(
		&mut self,
		lp: &LiquidityProvider,
		lower_tick: Tick,
		upper_tick: Tick,
	) -> Result<(Collected, PositionInfo), PositionError<CollectError>> {
		Self::validate_position_range(lower_tick, upper_tick)?;
		if let Some(mut position) =
			self.positions.get(&(lp.clone(), lower_tick, upper_tick)).cloned()
		{
			assert!(position.liquidity != 0);
			let lower_delta = self.liquidity_map.get(&lower_tick).unwrap();
			let upper_delta = self.liquidity_map.get(&upper_tick).unwrap();

			let collected_fees =
				position.collect_fees(self, lower_tick, lower_delta, upper_tick, upper_delta);
			let position_info = PositionInfo::from(&position);

			self.positions.insert((lp.clone(), lower_tick, upper_tick), position);

			Ok((collected_fees, position_info))
		} else {
			Err(PositionError::NonExistent)
		}
	}

	/// Swaps the specified Amount into the other currency until sqrt_price_limit is reached (If
	/// Some), and returns the resulting Amount and the remaining input Amount. The direction of the
	/// swap is controlled by the generic type parameter `SD`, by setting it to `ZeroToOne` or
	/// `OneToZero`.
	///
	/// This function never panics
	pub fn swap<SD: SwapDirection>(
		&mut self,
		mut amount: Amount,
		sqrt_price_limit: Option<U256>,
	) -> (Amount, Amount) {
		let mut total_output_amount = Amount::zero();

		// DIFF: This behaviour is different than Uniswap's. As Solidity doesn't have an ordered map
		// container, there is a fixed limit to how far the price can move in one iteration of the
		// loop, we don't have this restriction here.
		while let Some((tick_at_delta, delta)) = (!amount.is_zero() &&
			sqrt_price_limit.map_or(true, |sqrt_price_limit| {
				SD::sqrt_price_op_more_than(sqrt_price_limit, self.current_sqrt_price)
			}))
		.then_some(())
		.and_then(|()| SD::next_liquidity_delta(self.current_tick, &mut self.liquidity_map))
		{
			let sqrt_price_at_delta = sqrt_price_at_tick(*tick_at_delta);

			let sqrt_price_target = if let Some(sqrt_price_limit) = sqrt_price_limit {
				if SD::sqrt_price_op_more_than(sqrt_price_at_delta, sqrt_price_limit) {
					sqrt_price_limit
				} else {
					sqrt_price_at_delta
				}
			} else {
				sqrt_price_at_delta
			};

			let sqrt_price_next = if self.current_liquidity == 0 {
				sqrt_price_target
			} else {
				let amount_minus_fees = mul_div_floor(
					amount,
					U256::from(ONE_IN_HUNDREDTH_PIPS - self.fee_hundredth_pips),
					U256::from(ONE_IN_HUNDREDTH_PIPS),
				); // This cannot overflow as we bound fee_hundredth_pips to <= ONE_IN_HUNDREDTH_PIPS/2

				let amount_required_to_reach_target = SD::input_amount_delta_ceil(
					self.current_sqrt_price,
					sqrt_price_target,
					self.current_liquidity,
				);

				let sqrt_price_next = if amount_minus_fees >= amount_required_to_reach_target {
					sqrt_price_target
				} else {
					SD::next_sqrt_price_from_input_amount(
						self.current_sqrt_price,
						self.current_liquidity,
						amount_minus_fees,
					)
				};

				// Cannot overflow as if the swap traversed all ticks (MIN_TICK to MAX_TICK
				// (inclusive)), assuming the maximum possible liquidity, total_output_amount would
				// still be below U256::MAX (See test `output_amounts_bounded`)
				total_output_amount += SD::output_amount_delta_floor(
					self.current_sqrt_price,
					sqrt_price_next,
					self.current_liquidity,
				);

				let (amount_swapped, fees) = if sqrt_price_next == sqrt_price_target {
					(
						amount_required_to_reach_target,
						/* Will not overflow as fee_hundredth_pips <= ONE_IN_HUNDREDTH_PIPS / 2 */
						mul_div_ceil(
							amount_required_to_reach_target,
							U256::from(self.fee_hundredth_pips),
							U256::from(ONE_IN_HUNDREDTH_PIPS - self.fee_hundredth_pips),
						),
					)
				} else {
					let amount_swapped = SD::input_amount_delta_ceil(
						self.current_sqrt_price,
						sqrt_price_next,
						self.current_liquidity,
					);

					(
						amount_swapped,
						/* Will not underflow due to rounding in flavor of the pool of
						 * sqrt_price_next. */
						amount - amount_swapped,
					)
				};

				// TODO: Prove this does not underflow
				amount -= amount_swapped + fees;

				// DIFF: This behaviour is different to Uniswap's, we saturate instead of
				// overflowing/bricking the pool. This means we just stop giving LPs fees, but
				// this is exceptionally unlikely to occur due to the how large the maximum
				// global_fee_growth value is. We also do this to avoid needing to consider the
				// case of reverting an extrinsic's mutations which is expensive in Substrate
				// based chains.
				self.global_fee_growth[SD::INPUT_SIDE] = self.global_fee_growth[SD::INPUT_SIDE]
					.saturating_add(mul_div_floor(
						fees,
						U256::from(1) << 128u32,
						self.current_liquidity,
					));

				sqrt_price_next
			};

			assert!(!SD::sqrt_price_op_more_than(sqrt_price_next, sqrt_price_at_delta));

			if sqrt_price_next == sqrt_price_at_delta {
				delta.fee_growth_outside = SideMap::default()
					.map(|side, ()| self.global_fee_growth[side] - delta.fee_growth_outside[side]);
				self.current_sqrt_price = sqrt_price_next;
				self.current_tick = SD::current_tick_after_crossing_tick(*tick_at_delta);

				// Addition is guaranteed to never overflow, see test `max_liquidity`
				self.current_liquidity = self
					.current_liquidity
					.checked_add_signed(SD::liquidity_delta_on_crossing_tick(delta))
					.unwrap();
			} else if self.current_sqrt_price != sqrt_price_next {
				self.current_sqrt_price = sqrt_price_next;
				self.current_tick = tick_at_sqrt_price(sqrt_price_next);
			}
		}

		(total_output_amount, amount)
	}

	fn validate_position_range<T>(
		lower_tick: Tick,
		upper_tick: Tick,
	) -> Result<(), PositionError<T>> {
		(lower_tick < upper_tick && MIN_TICK <= lower_tick && upper_tick <= MAX_TICK)
			.then_some(())
			.ok_or(PositionError::InvalidTickRange)
	}

	/// Returns the value of a range order, and the liquidity that would contribute to the current
	/// liquidity level given the current price.
	///
	/// This function never panics
	pub fn liquidity_to_amounts<const ROUND_UP: bool>(
		&self,
		liquidity: Liquidity,
		lower_tick: Tick,
		upper_tick: Tick,
	) -> Result<(SideMap<Amount>, Liquidity), LiquidityToAmountsError> {
		(liquidity <= MAX_TICK_GROSS_LIQUIDITY)
			.then_some(())
			.ok_or(LiquidityToAmountsError::InvalidLiquidityAmount)?;
		Self::validate_position_range::<Infallible>(lower_tick, upper_tick)
			.map_err(|_err| LiquidityToAmountsError::InvalidTickRange)?;
		Ok(self.inner_liquidity_to_amounts::<ROUND_UP>(liquidity, lower_tick, upper_tick))
	}

	fn inner_liquidity_to_amounts<const ROUND_UP: bool>(
		&self,
		liquidity: Liquidity,
		lower_tick: Tick,
		upper_tick: Tick,
	) -> (SideMap<Amount>, Liquidity) {
		if self.current_tick < lower_tick {
			(
				SideMap::from_array([
					(if ROUND_UP { zero_amount_delta_ceil } else { zero_amount_delta_floor })(
						sqrt_price_at_tick(lower_tick),
						sqrt_price_at_tick(upper_tick),
						liquidity,
					),
					0.into(),
				]),
				0,
			)
		} else if self.current_tick < upper_tick {
			(
				SideMap::from_array([
					(if ROUND_UP { zero_amount_delta_ceil } else { zero_amount_delta_floor })(
						self.current_sqrt_price,
						sqrt_price_at_tick(upper_tick),
						liquidity,
					),
					(if ROUND_UP { one_amount_delta_ceil } else { one_amount_delta_floor })(
						sqrt_price_at_tick(lower_tick),
						self.current_sqrt_price,
						liquidity,
					),
				]),
				liquidity,
			)
		} else {
			(
				SideMap::from_array([
					0.into(),
					(if ROUND_UP { one_amount_delta_ceil } else { one_amount_delta_floor })(
						sqrt_price_at_tick(lower_tick),
						sqrt_price_at_tick(upper_tick),
						liquidity,
					),
				]),
				0,
			)
		}
	}

	fn size_as_liquidity(
		&self,
		lower_tick: Tick,
		upper_tick: Tick,
		size: Size,
	) -> Option<Liquidity> {
		match size {
			Size::Liquidity { liquidity } => Some(liquidity),
			Size::Amount { maximum, minimum } => {
				let liquidity = self.inner_amounts_to_liquidity(lower_tick, upper_tick, maximum);

				let (possible, _) =
					self.inner_liquidity_to_amounts::<false>(liquidity, lower_tick, upper_tick);

				if possible[Side::Zero] < minimum[Side::Zero] &&
					possible[Side::One] < minimum[Side::One]
				{
					None
				} else {
					Some(liquidity)
				}
			},
		}
	}

	fn inner_amounts_to_liquidity(
		&self,
		lower_tick: Tick,
		upper_tick: Tick,
		amounts: SideMap<Amount>,
	) -> Liquidity {
		// Inverse of `zero_amount_delta_ceil`
		fn zero_amount_to_liquidity(
			lower_sqrt_price: SqrtPriceQ64F96,
			upper_sqrt_price: SqrtPriceQ64F96,
			amounts: SideMap<Amount>,
		) -> U512 {
			(U512::saturating_mul(
				amounts[Side::Zero].into(),
				U256::full_mul(lower_sqrt_price, upper_sqrt_price),
			) / U512::from(upper_sqrt_price - lower_sqrt_price)) >>
				SQRT_PRICE_FRACTIONAL_BITS
		}

		// Inverse of `one_amount_delta_ceil`
		fn one_amount_to_liquidity(
			lower_sqrt_price: SqrtPriceQ64F96,
			upper_sqrt_price: SqrtPriceQ64F96,
			amounts: SideMap<Amount>,
		) -> U512 {
			U256::full_mul(amounts[Side::One], U256::one() << SQRT_PRICE_FRACTIONAL_BITS) /
				(upper_sqrt_price - lower_sqrt_price)
		}

		let [lower_sqrt_price, upper_sqrt_price] = [lower_tick, upper_tick].map(sqrt_price_at_tick);

		if self.current_sqrt_price <= lower_sqrt_price {
			zero_amount_to_liquidity(lower_sqrt_price, upper_sqrt_price, amounts)
		} else if self.current_sqrt_price < upper_sqrt_price {
			core::cmp::min(
				zero_amount_to_liquidity(self.current_sqrt_price, upper_sqrt_price, amounts),
				one_amount_to_liquidity(lower_sqrt_price, self.current_sqrt_price, amounts),
			)
		} else {
			one_amount_to_liquidity(lower_sqrt_price, upper_sqrt_price, amounts)
		}
		.try_into()
		.map(|liquidity| core::cmp::min(liquidity, MAX_TICK_GROSS_LIQUIDITY))
		.unwrap_or(MAX_TICK_GROSS_LIQUIDITY)
	}

	#[cfg(feature = "std")]
	pub fn positions(&self) -> BTreeMap<(LiquidityProvider, Tick, Tick), Liquidity> {
		self.positions.iter().map(|(k, v)| (k.clone(), v.liquidity)).collect()
	}
}

fn zero_amount_delta_floor(
	from: SqrtPriceQ64F96,
	to: SqrtPriceQ64F96,
	liquidity: Liquidity,
) -> Amount {
	assert!(SqrtPriceQ64F96::zero() < from);
	assert!(from <= to);

	/*
		Proof that `mul_div_floor` does not overflow:
		If A ∈ ℕ, B ∈ ℕ, A > 0, B >= A
		Then A * B >= B and B - A < B
		Then A * B > B - A
	*/
	mul_div_floor(
		U256::from(liquidity) << SQRT_PRICE_FRACTIONAL_BITS,
		to - from,
		U256::full_mul(to, from),
	)
}

fn zero_amount_delta_ceil(
	from: SqrtPriceQ64F96,
	to: SqrtPriceQ64F96,
	liquidity: Liquidity,
) -> Amount {
	assert!(SqrtPriceQ64F96::zero() < from);
	assert!(from <= to);

	/*
		Proof that `mul_div_ceil` does not overflow:
		If A ∈ ℕ, B ∈ ℕ, A > 0, B >= A
		Then A * B >= B and B - A < B
		Then A * B > B - A
	*/
	mul_div_ceil(
		U256::from(liquidity) << SQRT_PRICE_FRACTIONAL_BITS,
		to - from,
		U256::full_mul(to, from),
	)
}

fn one_amount_delta_floor(
	from: SqrtPriceQ64F96,
	to: SqrtPriceQ64F96,
	liquidity: Liquidity,
) -> Amount {
	assert!(from <= to);

	/*
		Proof that `mul_div_floor` does not overflow:
		If A ∈ u160, B ∈ u160, A <= B, L ∈ u128
		Then B - A ∈ u160
		Then (B - A) / (1<<96) <= u64::MAX (160 - 96 = 64)
		Then L * ((B - A) / (1<<96)) <= u192::MAX < u256::MAX
	*/
	mul_div_floor(liquidity.into(), to - from, U512::from(1) << SQRT_PRICE_FRACTIONAL_BITS)
}

fn one_amount_delta_ceil(
	from: SqrtPriceQ64F96,
	to: SqrtPriceQ64F96,
	liquidity: Liquidity,
) -> Amount {
	assert!(from <= to);

	/*
		Proof that `mul_div_ceil` does not overflow:
		If A ∈ u160, B ∈ u160, A <= B, L ∈ u128
		Then B - A ∈ u160
		Then (B - A) / (1<<96) <= u64::MAX (160 - 96 = 64)
		Then L * ((B - A) / (1<<96)) <= u192::MAX < u256::MAX
	*/
	mul_div_ceil(liquidity.into(), to - from, U512::from(1u32) << SQRT_PRICE_FRACTIONAL_BITS)
}
