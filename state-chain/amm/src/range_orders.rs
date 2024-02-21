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

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};
use sp_std::{collections::btree_map::BTreeMap, convert::Infallible, vec::Vec};

use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{U256, U512};

use crate::common::{
	is_sqrt_price_valid, is_tick_valid, mul_div_ceil, mul_div_floor, sqrt_price_at_tick,
	tick_at_sqrt_price, Amount, BaseToQuote, Pairs, PoolPairsMap, QuoteToBase, SetFeesError,
	SqrtPriceQ64F96, Tick, MAX_LP_FEE, MAX_TICK, MIN_TICK, ONE_IN_HUNDREDTH_PIPS,
	SQRT_PRICE_FRACTIONAL_BITS,
};

/// This is the invariant wrt xy = k. It represents / is proportional to the depth of the
/// pool/position.
pub type Liquidity = u128;
type FeeGrowthQ128F128 = U256;

/// This is the maximum Liquidity that can be associated with a given tick. Note this doesn't mean
/// the maximum amount of Liquidity a tick can have, but is the maximum allowed value of the sum of
/// the liquidity associated with all range orders that start or end at this tick.
/// This does indirectly limit the maximum liquidity at any price/tick, due to the fact there is
/// also a finite number of ticks i.e. all those in MIN_TICK..MAX_TICK. This limit exists to ensure
/// the output amount of a swap will never overflow a U256, even if the swap used all the liquidity
/// in the pool.
pub const MAX_TICK_GROSS_LIQUIDITY: Liquidity =
	Liquidity::MAX / ((1 + MAX_TICK - MIN_TICK) as u128);

#[derive(Clone, Debug, TypeInfo, Encode, Decode, MaxEncodedLen, Serialize, Deserialize)]
pub struct Position {
	/// The `depth` of this range order, this value is proportional to the value of the order i.e.
	/// the amount of assets that make up the order.
	liquidity: Liquidity,
	last_fee_growth_inside: PoolPairsMap<FeeGrowthQ128F128>,
	accumulative_fees: PoolPairsMap<Amount>,
	original_sqrt_price: SqrtPriceQ64F96,
}

impl Position {
	fn collect_fees<LiquidityProvider: Ord>(
		&mut self,
		pool_state: &PoolState<LiquidityProvider>,
		lower_tick: Tick,
		lower_delta: &TickDelta,
		upper_tick: Tick,
		upper_delta: &TickDelta,
	) -> Collected {
		let fee_growth_inside = PoolPairsMap::default().map_with_pair(|side, ()| {
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
		let fees = PoolPairsMap::default().map_with_pair(|side, ()| {
			// DIFF: This behaviour is different than Uniswap's. We use U256 instead of u128 to
			// calculate fees, therefore it is not possible to overflow the fees here.

			/*
				Proof that `mul_div_floor` does not overflow:
				Note position.liquidity: u128
				U512::one() << 128 > u128::MAX
			*/
			mul_div_floor(
				fee_growth_inside[side] - self.last_fee_growth_inside[side],
				self.liquidity.into(),
				U512::one() << 128,
			)
		});
		self.accumulative_fees = self
			.accumulative_fees
			.map_with_pair(|side, accumulative_fees| accumulative_fees.saturating_add(fees[side]));
		let collected_fees = Collected {
			fees,
			accumulative_fees: self.accumulative_fees,
			original_sqrt_price: self.original_sqrt_price,
		};
		self.last_fee_growth_inside = fee_growth_inside;
		collected_fees
	}

	fn set_liquidity<LiquidityProvider: Ord>(
		&mut self,
		pool_state: &PoolState<LiquidityProvider>,
		new_liquidity: Liquidity,
		lower_tick: Tick,
		lower_delta: &TickDelta,
		upper_tick: Tick,
		upper_delta: &TickDelta,
	) -> (Collected, PositionInfo) {
		// Before you can change the liquidity of a Position you must collect_fees, as the
		// `last_fee_growth_inside` member (which is used to calculate earned fees) is only
		// meaningful while liquidity is constant.
		let collected_fees =
			self.collect_fees(pool_state, lower_tick, lower_delta, upper_tick, upper_delta);
		if self.liquidity != new_liquidity {
			self.liquidity = new_liquidity;
			self.original_sqrt_price = pool_state.current_sqrt_price;
			self.accumulative_fees = Default::default();
		}
		(collected_fees, PositionInfo::from(&*self))
	}
}

#[derive(Clone, Debug, TypeInfo, Encode, Decode, MaxEncodedLen, Serialize, Deserialize)]
pub struct TickDelta {
	/// This is the change in the total amount of liquidity in the pool at this price, i.e. if the
	/// price moves from a lower price to a higher one, above this tick (higher/lower in literal
	/// integer value), the liquidity will increase by `liquidity_delta` and therefore swaps (In
	/// both directions) will experience less slippage (Assuming liquidity_delta is positive).
	liquidity_delta: i128,
	/// This is the sum of the liquidity of all the orders that start or end at this tick. Note
	/// this is the value that MAX_TICK_GROSS_LIQUIDITY applies to.
	liquidity_gross: u128,
	/// This is the fees per unit liquidity earned over all time while the current/swapping price
	/// was on the opposite side of this tick than it is at the moment. This can be used to
	/// calculate the fees earned by an order. It is stored this way as this value will only change
	/// when the price moves across this tick, thereby limiting the computation/state changes
	/// needed during a swap.
	fee_growth_outside: PoolPairsMap<FeeGrowthQ128F128>,
}

#[derive(Clone, Debug, TypeInfo, Encode, Decode, Serialize, Deserialize)]
pub struct PoolState<LiquidityProvider: Ord> {
	/// The percentage fee taken from swap inputs and earned by LPs. It is in units of 0.0001%.
	/// I.e. 5000 means 0.5%.
	pub(super) fee_hundredth_pips: u32,
	/// Note the current_sqrt_price can reach MAX_SQRT_PRICE, but only if the tick is MAX_TICK
	current_sqrt_price: SqrtPriceQ64F96,
	/// This is the highest tick that represents a strictly lower price than the
	/// current_sqrt_price. `current_tick` is the tick that when you swap BaseToQuote the
	/// `current_sqrt_price` is moving towards (going down in literal value), and will cross when
	/// `current_sqrt_price` reaches it. `current_tick + 1` is the tick the price is moving towards
	/// (going up in literal value) when you swap QuoteToBase and will cross when
	/// `current_sqrt_price` reaches it,
	current_tick: Tick,
	/// The total liquidity/depth at the `current_sqrt_price`
	current_liquidity: Liquidity,
	/// The total fees earned over all time per unit liquidity
	global_fee_growth: PoolPairsMap<FeeGrowthQ128F128>,
	/// All the ticks that have at least one range order that starts or ends at it, i.e. those
	/// ticks where liquidity_gross is non-zero.
	liquidity_map: BTreeMap<Tick, TickDelta>,
	positions: BTreeMap<(LiquidityProvider, Tick, Tick), Position>,
	/// Total fees earned over all time
	pub(super) total_fees_earned: PoolPairsMap<Amount>,
	/// Total of all swap inputs over all time (not including fees)
	pub(super) total_swap_inputs: PoolPairsMap<Amount>,
	/// Total of all swap outputs over all time
	total_swap_outputs: PoolPairsMap<Amount>,
}

pub(super) trait SwapDirection: crate::common::SwapDirection {
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

impl SwapDirection for BaseToQuote {
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

impl SwapDirection for QuoteToBase {
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
pub enum RequiredAssetRatioError {
	/// Invalid Tick range
	InvalidTickRange,
}

#[derive(Debug)]
pub enum DepthError {
	/// Invalid Price
	InvalidTick,
	/// Start tick must be less than or equal to the end tick
	InvalidTickRange,
}

#[derive(Debug)]
pub enum LiquidityToAmountsError {
	/// Invalid Tick range
	InvalidTickRange,
	/// `liquidity` is larger than the maximum
	LiquidityTooLarge,
}

#[derive(Default, Debug, PartialEq, Eq, TypeInfo, Encode, Decode, MaxEncodedLen)]
pub struct Collected {
	pub fees: PoolPairsMap<Amount>,
	pub accumulative_fees: PoolPairsMap<Amount>,
	pub original_sqrt_price: SqrtPriceQ64F96,
}

#[derive(Debug, PartialEq, Eq, TypeInfo, Encode, Decode, MaxEncodedLen)]
pub enum Size {
	Liquidity { liquidity: Liquidity },
	Amount { maximum: PoolPairsMap<Amount>, minimum: PoolPairsMap<Amount> },
}

#[derive(Default, Debug, PartialEq, Eq, TypeInfo, Encode, Decode, MaxEncodedLen)]
pub struct PositionInfo {
	pub liquidity: Liquidity,
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
	pub(super) fn new(fee_hundredth_pips: u32, initial_sqrt_price: U256) -> Result<Self, NewError> {
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
			total_fees_earned: Default::default(),
			total_swap_inputs: Default::default(),
			total_swap_outputs: Default::default(),
		})
	}

	pub(super) fn collect_all(
		&mut self,
	) -> impl '_ + Iterator<Item = ((LiquidityProvider, Tick, Tick), (Collected, PositionInfo))> {
		self.positions.keys().cloned().collect::<sp_std::vec::Vec<_>>().into_iter().map(
			|(lp, lower_tick, upper_tick)| {
				(
					(lp.clone(), lower_tick, upper_tick),
					self.collect(&lp, lower_tick, upper_tick).unwrap(),
				)
			},
		)
	}

	/// Sets the fee for the pool. This will apply to future swaps. This function will fail if the
	/// fee is greater than 50%.
	///
	/// This function never panics
	pub(super) fn set_fees(&mut self, fee_hundredth_pips: u32) -> Result<(), SetFeesError> {
		Self::validate_fees(fee_hundredth_pips)
			.then_some(())
			.ok_or(SetFeesError::InvalidFeeAmount)?;
		self.fee_hundredth_pips = fee_hundredth_pips;
		Ok(())
	}

	pub fn validate_fees(fee_hundredth_pips: u32) -> bool {
		fee_hundredth_pips <= MAX_LP_FEE
	}

	/// Returns the current sqrt price of the pool. None if the pool has no more liquidity and the
	/// price cannot get worse.
	///
	/// This function never panics
	pub(super) fn current_sqrt_price<SD: SwapDirection>(&self) -> Option<SqrtPriceQ64F96> {
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
	pub(super) fn collect_and_mint<T, E, TryDebit: FnOnce(PoolPairsMap<Amount>) -> Result<T, E>>(
		&mut self,
		lp: &LiquidityProvider,
		lower_tick: Tick,
		upper_tick: Tick,
		size: Size,
		try_debit: TryDebit,
	) -> Result<(T, Liquidity, Collected, PositionInfo), PositionError<MintError<E>>> {
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
				accumulative_fees: Default::default(),
				original_sqrt_price: self.current_sqrt_price,
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

			Ok((t, minted_liquidity, collected_fees, position_info))
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
	pub(super) fn collect_and_burn(
		&mut self,
		lp: &LiquidityProvider,
		lower_tick: Tick,
		upper_tick: Tick,
		size: Size,
	) -> Result<(PoolPairsMap<Amount>, Liquidity, Collected, PositionInfo), PositionError<BurnError>>
	{
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
			// current_liquidity for it to need to be subtracted now
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
			Ok((amounts_owed, burnt_liquidity, collected_fees, position_info))
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
	#[allow(dead_code)]
	pub(super) fn collect(
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
	/// swap is controlled by the generic type parameter `SD`, by setting it to `BaseToQuote` or
	/// `QuoteToBase`.
	///
	/// This function never panics
	pub(super) fn swap<SD: SwapDirection>(
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

				self.total_swap_inputs[SD::INPUT_SIDE] =
					self.total_swap_inputs[SD::INPUT_SIDE].saturating_add(amount_swapped);
				self.total_fees_earned[SD::INPUT_SIDE] =
					self.total_fees_earned[SD::INPUT_SIDE].saturating_add(fees);

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
				delta.fee_growth_outside = PoolPairsMap::default().map_with_pair(|side, ()| {
					self.global_fee_growth[side] - delta.fee_growth_outside[side]
				});
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

		self.total_swap_outputs[!SD::INPUT_SIDE] =
			self.total_swap_outputs[!SD::INPUT_SIDE].saturating_add(total_output_amount);

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

	/// Returns the ratio of assets required to create a range order at the given tick range
	///
	/// This function never panics
	pub(super) fn required_asset_ratio<const ROUND_UP: bool>(
		&self,
		lower_tick: Tick,
		upper_tick: Tick,
	) -> Result<PoolPairsMap<Amount>, RequiredAssetRatioError> {
		Self::validate_position_range::<Infallible>(lower_tick, upper_tick)
			.map_err(|_err| RequiredAssetRatioError::InvalidTickRange)?;
		Ok(self
			.inner_liquidity_to_amounts::<ROUND_UP>(
				MAX_TICK_GROSS_LIQUIDITY,
				lower_tick,
				upper_tick,
			)
			.0)
	}

	/// Returns the value of a range order with a given amount of liquidity, i.e. the assets that
	/// you would need to create such as position, or that you would get if such a position was
	/// burned.
	///
	/// This function never panics
	pub(super) fn liquidity_to_amounts<const ROUND_UP: bool>(
		&self,
		liquidity: Liquidity,
		lower_tick: Tick,
		upper_tick: Tick,
	) -> Result<PoolPairsMap<Amount>, LiquidityToAmountsError> {
		Self::validate_position_range::<Infallible>(lower_tick, upper_tick)
			.map_err(|_err| LiquidityToAmountsError::InvalidTickRange)?;
		if liquidity > MAX_TICK_GROSS_LIQUIDITY {
			Err(LiquidityToAmountsError::LiquidityTooLarge)
		} else {
			Ok(self.inner_liquidity_to_amounts::<ROUND_UP>(liquidity, lower_tick, upper_tick).0)
		}
	}

	fn inner_liquidity_to_amounts<const ROUND_UP: bool>(
		&self,
		liquidity: Liquidity,
		lower_tick: Tick,
		upper_tick: Tick,
	) -> (PoolPairsMap<Amount>, Liquidity) {
		if self.current_tick < lower_tick {
			(
				PoolPairsMap::from_array([
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
				PoolPairsMap::from_array([
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
				PoolPairsMap::from_array([
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

				if possible[Pairs::Base] < minimum[Pairs::Base] ||
					possible[Pairs::Quote] < minimum[Pairs::Quote]
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
		amounts: PoolPairsMap<Amount>,
	) -> Liquidity {
		// Inverse of `zero_amount_delta_ceil`
		fn zero_amount_to_liquidity(
			lower_sqrt_price: SqrtPriceQ64F96,
			upper_sqrt_price: SqrtPriceQ64F96,
			amounts: PoolPairsMap<Amount>,
		) -> U512 {
			(U512::saturating_mul(
				amounts[Pairs::Base].into(),
				U256::full_mul(lower_sqrt_price, upper_sqrt_price),
			) / U512::from(upper_sqrt_price - lower_sqrt_price)) >>
				SQRT_PRICE_FRACTIONAL_BITS
		}

		// Inverse of `one_amount_delta_ceil`
		fn one_amount_to_liquidity(
			lower_sqrt_price: SqrtPriceQ64F96,
			upper_sqrt_price: SqrtPriceQ64F96,
			amounts: PoolPairsMap<Amount>,
		) -> U512 {
			U256::full_mul(amounts[Pairs::Quote], U256::one() << SQRT_PRICE_FRACTIONAL_BITS) /
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

	/// Returns an iterator over all positions
	///
	/// This function never panics.
	pub(super) fn positions(
		&self,
	) -> impl '_ + Iterator<Item = (LiquidityProvider, Tick, Tick, Collected, PositionInfo)> {
		self.positions.iter().map(|((lp, lower_tick, upper_tick), position)| {
			let mut position = position.clone();
			(
				lp.clone(),
				*lower_tick,
				*upper_tick,
				position.collect_fees(
					self,
					*lower_tick,
					self.liquidity_map.get(lower_tick).unwrap(),
					*upper_tick,
					self.liquidity_map.get(upper_tick).unwrap(),
				),
				PositionInfo::from(&position),
			)
		})
	}

	/// Returns the current value of a position i.e. the assets you would receive by burning the
	/// position, and the fees earned by the position since the last time it was updated/collected.
	///
	/// This function never panics
	pub(super) fn position(
		&self,
		lp: &LiquidityProvider,
		lower_tick: Tick,
		upper_tick: Tick,
	) -> Result<(Collected, PositionInfo), PositionError<Infallible>> {
		Self::validate_position_range(lower_tick, upper_tick)?;
		let mut position = self
			.positions
			.get(&(lp.clone(), lower_tick, upper_tick))
			.ok_or(PositionError::NonExistent)?
			.clone();
		Ok((
			position.collect_fees(
				self,
				lower_tick,
				self.liquidity_map.get(&lower_tick).unwrap(),
				upper_tick,
				self.liquidity_map.get(&upper_tick).unwrap(),
			),
			PositionInfo::from(&position),
		))
	}

	/// Returns a histogram of all the liquidity in the pool. Each entry in the returned vec is the
	/// "start" tick, and the amount of liquidity in the pool from that tick, until the next tick,
	/// i.e. the next tick in the pool. The first element will always be the MIN_TICK with some
	/// amount of liquidity, and the last element will always be the MAX_TICK with a zero amount of
	/// liquidity.
	///
	/// This function never panics
	pub(super) fn liquidity(&self) -> Vec<(Tick, Liquidity)> {
		let mut liquidity = 0u128;
		self.liquidity_map
			.iter()
			.map(|(tick, tick_delta)| {
				liquidity = liquidity
					.checked_add_signed(QuoteToBase::liquidity_delta_on_crossing_tick(tick_delta))
					.unwrap();

				(*tick, liquidity)
			})
			.collect()
	}

	pub(super) fn depth(
		&self,
		lower_tick: Tick,
		upper_tick: Tick,
	) -> Result<PoolPairsMap<Amount>, DepthError> {
		if !is_tick_valid(lower_tick) || !is_tick_valid(upper_tick) {
			return Err(DepthError::InvalidTick)
		}

		if lower_tick <= upper_tick {
			let liquidity_at_lower_tick: Liquidity =
				self.liquidity_map.range(..lower_tick).fold(0, |liquidity, (_, tick_delta)| {
					liquidity.checked_add_signed(tick_delta.liquidity_delta).unwrap()
				});

			let (_liquidity, _tick, assets) = self
				.liquidity_map
				.range(lower_tick..upper_tick)
				.map(|(tick, tick_delta)| (tick, tick_delta.liquidity_delta))
				.chain(core::iter::once((&upper_tick, 0 /* value doesn't matter */)))
				.fold(
					(liquidity_at_lower_tick, lower_tick, PoolPairsMap::<Amount>::default()),
					|(liquidity, previous_tick, assets), (current_tick, liquidity_delta)| {
						(
							// Addition is guaranteed to never overflow, see test `max_liquidity`
							liquidity.checked_add_signed(liquidity_delta).unwrap(),
							*current_tick,
							assets +
								self.inner_liquidity_to_amounts::<false>(
									liquidity,
									previous_tick,
									*current_tick,
								)
								.0,
						)
					},
				);

			Ok(assets)
		} else {
			Err(DepthError::InvalidTickRange)
		}
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
