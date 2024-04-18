//! This code implements a single liquidity pool pair, that allows LPs to specify particular prices
//! at with they want to sell one of the two assets in the pair. The price an LP wants to sell at
//! is specified using `Tick`s.
//!
//! This type of pool doesn't do automated market making, as in the price of the pool is purely
//! determined be the best priced position currently in the pool.
//!
//! Swaps in this pool will execute on the best priced positions first. Note if two positions
//! have the same price, both positions will be partially executed, and neither will receive
//! "priority" regardless of when they were created, i.e. an equal percentage of all positions at
//! the same price will be executed. So larger positions will earn more fees (and the absolute
//! amount of the position that is executed will be greater, but the same percentage-wise) as they
//! contribute more to the swap.
//!
//! To track fees earned and remaining liquidity in each position, the pool records the big product
//! of the "percent_remaining" of each swap. Using two of these values you can calculate the
//! percentage of liquidity swapped in a position between the two points in time at which those
//! percent_remaining values were recorded.

#[cfg(test)]
mod tests;

use core::convert::Infallible;

use serde::{Deserialize, Serialize};
use sp_std::collections::btree_map::BTreeMap;

use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{U256, U512};
use sp_std::vec::Vec;

use crate::common::{
	is_tick_valid, mul_div_ceil, mul_div_floor, sqrt_price_at_tick, sqrt_price_to_price,
	tick_at_sqrt_price, Amount, BaseToQuote, PoolPairsMap, Price, QuoteToBase, SetFeesError,
	SqrtPriceQ64F96, Tick, MAX_LP_FEE, ONE_IN_HUNDREDTH_PIPS, PRICE_FRACTIONAL_BITS,
};

// This is the maximum liquidity/amount of an asset that can be sold at a single tick/price. If an
// LP attempts to add more liquidity that would increase the total at the tick past this value, the
// minting operation will error. Note this maximum is for all lps combined, and not a single lp,
// therefore it is possible for an LP to "consume" a tick by filling it up to the maximum, and
// thereby not allowing other LPs to mint at that price (But the maximum is high enough that this is
// not feasible).
const MAX_FIXED_POOL_LIQUIDITY: Amount = U256([u64::MAX, u64::MAX, 0, 0] /* little endian */);

/// Represents a number exclusively between 0 and 1.
#[derive(
	Clone, Debug, PartialEq, Eq, TypeInfo, Encode, Decode, MaxEncodedLen, Serialize, Deserialize,
)]
#[cfg_attr(feature = "std", derive(Default))]
struct FloatBetweenZeroAndOne {
	/// A fixed point number where the msb has a value of `0.5`,
	/// therefore it cannot represent 1.0, only numbers inside
	/// `0.0..1.0`, although note the mantissa will never be zero, and
	/// this is enforced by the public functions of the type. We also
	/// enforce that the top bit of the mantissa is always set, i.e.
	/// the float point number is `normalised`. Therefore the mantissa
	/// always has a value between `0.5..1.0`.
	normalised_mantissa: U256,
	/// As we are only interested in representing real numbers below 1,
	/// the exponent is either 0 or negative.                                   
	negative_exponent: U256,
}
impl Ord for FloatBetweenZeroAndOne {
	fn cmp(&self, other: &Self) -> core::cmp::Ordering {
		// Because the float is normalised we can get away with comparing only the exponents (unless
		// they are the same). Also note the exponent comparison is reversed, as the exponent is
		// implicitly negative.
		other
			.negative_exponent
			.cmp(&self.negative_exponent)
			.then_with(|| self.normalised_mantissa.cmp(&other.normalised_mantissa))
	}
}
impl PartialOrd for FloatBetweenZeroAndOne {
	fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
		Some(self.cmp(other))
	}
}
impl FloatBetweenZeroAndOne {
	/// Returns the largest possible value i.e. `1.0 - (2^-256)`.
	fn max() -> Self {
		Self { normalised_mantissa: U256::max_value(), negative_exponent: U256::zero() }
	}

	/// Rights shifts x by shift_bits bits, returning the result and the bits that were shifted
	/// out/the remainder. You can think of this as a div_mod, but we are always dividing by powers
	/// of 2.
	fn right_shift_mod(x: U512, shift_bits: U256) -> (U512, U512) {
		if shift_bits >= U256::from(512) {
			(U512::zero(), x)
		} else {
			let shift_bits = shift_bits.as_u32();
			(x >> shift_bits, x & (U512::MAX >> (512 - shift_bits)))
		}
	}

	/// Returns the result of `self * numerator / denominator` with the result rounded up.
	///
	/// This function will panic if the numerator is zero, or if numerator > denominator
	fn mul_div_ceil(&self, numerator: U256, denominator: U256) -> Self {
		// We cannot use the `mul_div_ceil` function here (and then right-shift the result) to
		// calculate the normalised_mantissa as the low zero bits (where we shifted) could be wrong.

		assert!(!numerator.is_zero());
		assert!(numerator <= denominator);
		self.assert_valid();

		// We do the mul first to avoid losing precision as in the division bits will possibly get
		// shifted off the "bottom" of the mantissa.
		let (mul_normalised_mantissa, mul_normalise_shift) = {
			let unnormalised_mantissa = U256::full_mul(self.normalised_mantissa, numerator);
			let normalize_shift = unnormalised_mantissa.leading_zeros();
			(
				unnormalised_mantissa << normalize_shift,
				256 - normalize_shift, /* Cannot underflow as numerator != 0 */
			)
		};

		let (mul_div_normalised_mantissa, div_normalise_shift) = {
			// As the denominator <= U256::MAX, this div will not right-shift the mantissa more than
			// 256 bits, so we maintain at least 256 accurate bits in the result.
			let (d, div_remainder) =
				U512::div_mod(mul_normalised_mantissa, U512::from(denominator)); // Note that d can never be zero as mul_normalised_mantissa always has at least one bit
																 // set above the 256th bit.
			let d = if div_remainder.is_zero() { d } else { d + U512::one() };
			let normalise_shift = d.leading_zeros();
			// We right shift and use the lower 256 bits for the mantissa
			let shift_bits = 256 - normalise_shift;
			let (d, shift_remainder) = Self::right_shift_mod(d, shift_bits.into());
			let d = U256::try_from(d).unwrap();

			(if shift_remainder.is_zero() { d } else { d + U256::one() }, normalise_shift)
		};

		assert!(!mul_div_normalised_mantissa.is_zero());

		if let Some(negative_exponent) = self
			.negative_exponent
			.checked_add(U256::from(div_normalise_shift - mul_normalise_shift))
		{
			Self { normalised_mantissa: mul_div_normalised_mantissa, negative_exponent }
		} else {
			// This bounding will cause swaps to get bad prices, but this case will effectively
			// never happen, as at least (U256::MAX / 256) (~10^74) swaps would have to happen to
			// get into this situation. TODO: A possible solution is disabling minting for pools
			// "close" to this minimum. With a small change to the swapping logic it would be
			// possible to guarantee that the pool would be emptied before percent_remaining could
			// reach this min bound.
			Self { normalised_mantissa: U256::one() << 255, negative_exponent: U256::MAX }
		}
	}

	/// Returns both floor and ceil of `y = x * numerator / denominator`.
	///
	/// This will panic if the numerator is more than the denominator.
	fn integer_mul_div(x: U256, numerator: &Self, denominator: &Self) -> (U256, U256) {
		// Note this does not imply numerator.normalised_mantissa <= denominator.normalised_mantissa
		assert!(numerator <= denominator);
		numerator.assert_valid();
		denominator.assert_valid();

		let (y_shifted_floor, div_remainder) = U512::div_mod(
			U256::full_mul(x, numerator.normalised_mantissa),
			denominator.normalised_mantissa.into(),
		);

		// Unwrap safe as numerator is smaller than denominator, so its negative_exponent must be
		// greater than or equal to the denominator's
		let negative_exponent =
			numerator.negative_exponent.checked_sub(denominator.negative_exponent).unwrap();

		let (y_floor, shift_remainder) = Self::right_shift_mod(y_shifted_floor, negative_exponent);

		let y_floor = y_floor.try_into().unwrap(); // Unwrap safe as numerator <= denominator and therefore y cannot be greater than x

		(
			y_floor,
			if div_remainder.is_zero() && shift_remainder.is_zero() {
				y_floor
			} else {
				y_floor + 1 // Safe as for there to be a remainder y_floor must be at least 1 less than x
			},
		)
	}

	fn assert_valid(&self) {
		assert!(self.normalised_mantissa.bit(255));
	}
}

pub(super) trait SwapDirection: crate::common::SwapDirection {
	/// Calculates the swap input amount needed to produce an output amount at a price
	fn input_amount_ceil(output: Amount, price: Price) -> Amount;

	/// Calculates the swap input amount needed to produce an output amount at a price
	fn input_amount_floor(output: Amount, price: Price) -> Amount;

	/// Calculates the swap output amount produced for an input amount at a price
	fn output_amount_floor(input: Amount, price: Price) -> Amount;

	/// Gets entry for best prices pool
	fn best_priced_fixed_pool(
		pools: &'_ mut BTreeMap<SqrtPriceQ64F96, FixedPool>,
	) -> Option<sp_std::collections::btree_map::OccupiedEntry<'_, SqrtPriceQ64F96, FixedPool>>;
}
impl SwapDirection for BaseToQuote {
	fn input_amount_ceil(output: Amount, price: Price) -> Amount {
		mul_div_ceil(output, U256::one() << PRICE_FRACTIONAL_BITS, price)
	}

	fn input_amount_floor(output: Amount, price: Price) -> Amount {
		QuoteToBase::output_amount_floor(output, price)
	}

	fn output_amount_floor(input: Amount, price: Price) -> Amount {
		mul_div_floor(input, price, U256::one() << PRICE_FRACTIONAL_BITS)
	}

	fn best_priced_fixed_pool(
		pools: &'_ mut BTreeMap<SqrtPriceQ64F96, FixedPool>,
	) -> Option<sp_std::collections::btree_map::OccupiedEntry<'_, SqrtPriceQ64F96, FixedPool>> {
		pools.last_entry()
	}
}
impl SwapDirection for QuoteToBase {
	fn input_amount_ceil(output: Amount, price: Price) -> Amount {
		mul_div_ceil(output, price, U256::one() << PRICE_FRACTIONAL_BITS)
	}

	fn input_amount_floor(output: Amount, price: Price) -> Amount {
		BaseToQuote::output_amount_floor(output, price)
	}

	fn output_amount_floor(input: Amount, price: Price) -> Amount {
		mul_div_floor(input, U256::one() << PRICE_FRACTIONAL_BITS, price)
	}

	fn best_priced_fixed_pool(
		pools: &'_ mut BTreeMap<SqrtPriceQ64F96, FixedPool>,
	) -> Option<sp_std::collections::btree_map::OccupiedEntry<'_, SqrtPriceQ64F96, FixedPool>> {
		pools.first_entry()
	}
}

#[derive(Debug)]
pub enum NewError {
	/// Fee must be between 0 - 50%
	InvalidFeeAmount,
}

#[derive(Debug)]
pub enum DepthError {
	/// Invalid Price
	InvalidTick,
	/// Start tick must be less than or equal to the end tick
	InvalidTickRange,
}

#[derive(Debug)]
pub enum MintError {
	/// One of the start/end ticks of the range reached its maximum gross liquidity
	MaximumLiquidity,
	/// Pool instance limit, this occurs when we run out of unique pool instance indices
	MaximumPoolInstances,
}

#[derive(Debug)]
pub enum PositionError<T> {
	/// Invalid Price
	InvalidTick,
	/// Position referenced does not exist
	NonExistent,
	Other(T),
}
impl<T> PositionError<T> {
	fn map_other<R>(self, f: impl FnOnce(T) -> R) -> PositionError<R> {
		match self {
			PositionError::InvalidTick => PositionError::InvalidTick,
			PositionError::NonExistent => PositionError::NonExistent,
			PositionError::Other(t) => PositionError::Other(f(t)),
		}
	}
}

#[derive(Debug)]
pub enum BurnError {}

#[derive(Debug)]
pub enum CollectError {}

#[derive(Default, Debug, PartialEq, Eq, TypeInfo, Encode, Decode, MaxEncodedLen)]
pub struct Collected {
	/// The amount of fees earned by this position since the last collect.
	pub fees: Amount,
	/// The amount of assets purchased by the LP using the liquidity in this position since the
	/// last collect.
	pub bought_amount: Amount,
	/// The amount of assets sold by the LP using the liquidity in this position since the
	/// last collect.
	pub sold_amount: Amount,
	/// The accumulative fees earned by this position since the last modification of the position
	/// i.e. non-zero mint or burn.
	pub accumulative_fees: Amount,
	/// The amount of liquidity in the position when it was created/last updated (a non-zero mint
	/// or burn) prior to the operation that which returned this value.
	pub original_amount: Amount,
}

#[derive(Default, Debug, PartialEq, Eq, TypeInfo, Encode, Decode, MaxEncodedLen)]
pub struct PositionInfo {
	/// The amount of liquidity in the position after the operation.
	pub amount: Amount,
}
impl PositionInfo {
	pub fn new(amount: Amount) -> Self {
		Self { amount }
	}
}
impl<'a> From<&'a Position> for PositionInfo {
	fn from(value: &'a Position) -> Self {
		Self { amount: value.amount }
	}
}

/// Represents a single LP position
#[derive(Clone, Debug, TypeInfo, Encode, Decode, MaxEncodedLen, Serialize, Deserialize)]
struct Position {
	/// Used to identify when the position was created and thereby determine if all the liquidity
	/// in the position has been used or not. As once all the liquidity at a tick has been used,
	/// the internal record of that tick/fixed pool is deleted, and if liquidity is added back
	/// later the record will have a different pool_instance. Therefore a position can tell if all
	/// its liquidity has been used, by seeing if there is not a fixed pool at the same tick, or if
	/// that fixed pool has a different pool_instance.
	pool_instance: u128,
	/// The total amount of liquidity provided by this position as of the last operation on the
	/// position. I.e. This value is not updated when swaps occur, only when the LP updates their
	/// position in some way.
	amount: Amount,
	/// This value is used in combination with the FixedPool's `percent_remaining` to determine how
	/// much liquidity/amount is remaining in a position when an LP does a collect/update of the
	/// position. It is the percent_remaining of the FixedPool when the position was last
	/// updated/collected from.
	last_percent_remaining: FloatBetweenZeroAndOne,
	/// This is the total fees earned by this position since the last non-zero mint or burn on the
	/// position, as of the last operation on the position. I.e. this value is not updated when
	/// swaps occur, only when the LP updates their position in some way.
	accumulative_fees: Amount,
	/// This is the original amount of liquidity provider by this position as of its creation. This
	/// value is updated if a non-zero mint or burn is performed on the position.
	original_amount: Amount,
}

/// Represents a pool that is selling an amount of an asset at a specific/fixed price. A
/// single fixed pool will contain the liquidity/assets for all limit orders at that specific price.
#[derive(Clone, Debug, TypeInfo, Encode, Decode, MaxEncodedLen, Serialize, Deserialize)]
pub(super) struct FixedPool {
	/// Whenever a FixedPool is destroyed and recreated i.e. all the liquidity in the FixedPool is
	/// used, a new value for pool_instance is used, and the previously used value will never be
	/// used again. This is used to determine whether a position was created during the current
	/// FixedPool's lifetime and therefore that FixedPool's `percent_remaining` is meaningful for
	/// the position, or if the position was created before the current FixedPool's lifetime.
	pool_instance: u128,
	/// This is the total liquidity/amount available for swaps at this price. This value is greater
	/// than or equal to the amount provided currently by all positions at the same tick. It is not
	/// always equal due to rounding, and therefore it is possible for a FixedPool to have no
	/// associated position but have some liquidity available, but this would likely be a very
	/// small amount.
	available: Amount,
	/// This is the big product of all `1.0 - percent_used_by_swap` for all swaps that have
	/// occurred since this FixedPool instance was created and used liquidity from it.
	percent_remaining: FloatBetweenZeroAndOne,
}

#[derive(Clone, Debug, TypeInfo, Encode, Decode, Serialize, Deserialize)]
pub(super) struct PoolState<LiquidityProvider: Ord> {
	/// The percentage fee taken from swap inputs and earned by LPs. It is in units of 0.0001%.
	/// I.e. 5000 means 0.5%.
	pub(super) fee_hundredth_pips: u32,
	/// The ID the next FixedPool that is created will use.
	next_pool_instance: u128,
	/// All the FixedPools that have some liquidity. They are grouped into all those that are
	/// selling asset `Base` and all those that are selling asset `Quote` used the PoolPairsMap.
	fixed_pools: PoolPairsMap<BTreeMap<SqrtPriceQ64F96, FixedPool>>,
	/// All the Positions that either are providing liquidity currently, or were providing
	/// liquidity directly after the last time they where updated. They are grouped into all those
	/// that are selling asset `Base` and all those that are selling asset `Quote` used the
	/// PoolPairsMap. Therefore there can be positions stored here that don't provide any
	/// liquidity.
	positions: PoolPairsMap<BTreeMap<(SqrtPriceQ64F96, LiquidityProvider), Position>>,
	/// Total fees earned over all time
	pub(super) total_fees_earned: PoolPairsMap<Amount>,
	/// Total of all swap inputs over all time (not including fees)
	pub(super) total_swap_inputs: PoolPairsMap<Amount>,
	/// Total of all swap outputs over all time
	total_swap_outputs: PoolPairsMap<Amount>,
}

impl<LiquidityProvider: Clone + Ord> PoolState<LiquidityProvider> {
	/// Creates a new pool state with the given fee. The pool is created with no liquidity. The pool
	/// may not be created with a fee higher than 50%.
	///
	/// This function never panics.
	pub(super) fn new(fee_hundredth_pips: u32) -> Result<Self, NewError> {
		Self::validate_fees(fee_hundredth_pips)
			.then_some(())
			.ok_or(NewError::InvalidFeeAmount)?;

		Ok(Self {
			fee_hundredth_pips,
			next_pool_instance: 0,
			fixed_pools: Default::default(),
			positions: Default::default(),
			total_fees_earned: Default::default(),
			total_swap_inputs: Default::default(),
			total_swap_outputs: Default::default(),
		})
	}

	/// Creates an iterator over all positions
	///
	/// This function never panics.
	pub(super) fn positions<SD: SwapDirection>(
		&self,
	) -> impl '_ + Iterator<Item = (LiquidityProvider, Tick, Collected, PositionInfo)> {
		self.positions[!SD::INPUT_SIDE].iter().map(|((sqrt_price, lp), position)| {
			let (collected, option_position) = Self::collect_from_position::<SD>(
				position.clone(),
				self.fixed_pools[!SD::INPUT_SIDE].get(sqrt_price),
				sqrt_price_to_price(*sqrt_price),
				self.fee_hundredth_pips,
			);

			(
				lp.clone(),
				tick_at_sqrt_price(*sqrt_price),
				collected,
				option_position
					.map_or(Default::default(), |position| PositionInfo::from(&position)),
			)
		})
	}

	/// Runs collect for all positions in the pool. Returns a PoolPairsMap
	/// containing the state and fees collected from every position. The positions are grouped into
	/// a PoolPairsMap by the asset they sell.
	///
	/// This function never panics.
	#[allow(clippy::type_complexity)]
	pub(super) fn collect_all(
		&mut self,
	) -> PoolPairsMap<Vec<(LiquidityProvider, Tick, Collected, PositionInfo)>> {
		// We must collect all positions before we can change the fee, otherwise the fee and swapped
		// liquidity calculations would be wrong.
		PoolPairsMap::from_array([
			self.positions[!<QuoteToBase as crate::common::SwapDirection>::INPUT_SIDE]
				.keys()
				.cloned()
				.collect::<sp_std::vec::Vec<_>>()
				.into_iter()
				.map(|(sqrt_price, lp)| {
					let (collected, position_info) =
						self.inner_collect::<QuoteToBase>(&lp, sqrt_price).unwrap();

					(lp.clone(), tick_at_sqrt_price(sqrt_price), collected, position_info)
				})
				.collect(),
			self.positions[!<BaseToQuote as crate::common::SwapDirection>::INPUT_SIDE]
				.keys()
				.cloned()
				.collect::<sp_std::vec::Vec<_>>()
				.into_iter()
				.map(|(sqrt_price, lp)| {
					let (collected, position_info) =
						self.inner_collect::<BaseToQuote>(&lp, sqrt_price).unwrap();

					(lp.clone(), tick_at_sqrt_price(sqrt_price), collected, position_info)
				})
				.collect(),
		])
	}

	/// Sets the fee for the pool. This will apply to future swaps. The fee may not be set
	/// higher than 50%. Also runs collect for all positions in the pool. Returns a PoolPairsMap
	/// containing the state and fees collected from every position as part of the set_fees
	/// operation. The positions are grouped into a PoolPairsMap by the asset they sell.
	///
	/// This function never panics.
	#[allow(clippy::type_complexity)]
	pub(super) fn set_fees(
		&mut self,
		fee_hundredth_pips: u32,
	) -> Result<PoolPairsMap<Vec<(LiquidityProvider, Tick, Collected, PositionInfo)>>, SetFeesError>
	{
		Self::validate_fees(fee_hundredth_pips)
			.then_some(())
			.ok_or(SetFeesError::InvalidFeeAmount)?;

		let collect_all = self.collect_all();
		self.fee_hundredth_pips = fee_hundredth_pips;
		Ok(collect_all)
	}

	/// Returns the current price of the pool for a given swap direction, if some liquidity exists.
	///
	/// This function never panics.
	pub(super) fn current_sqrt_price<SD: SwapDirection>(&mut self) -> Option<SqrtPriceQ64F96> {
		SD::best_priced_fixed_pool(&mut self.fixed_pools[!SD::INPUT_SIDE]).map(|entry| *entry.key())
	}

	/// Swaps the specified Amount into the other currency until sqrt_price_limit is reached (If
	/// Some), and returns the resulting Amount and the remaining input Amount. The direction of the
	/// swap is controlled by the generic type parameter `SD`, by setting it to `BaseToQuote` or
	/// `QuoteToBase`. Note sqrt_price_limit is inclusive.
	///
	/// This function never panics
	pub(super) fn swap<SD: SwapDirection>(
		&mut self,
		mut amount: Amount,
		sqrt_price_limit: Option<SqrtPriceQ64F96>,
	) -> (Amount, Amount) {
		let mut total_output_amount = U256::zero();

		while let Some((sqrt_price, mut fixed_pool_entry)) = (!amount.is_zero())
			.then_some(())
			.and_then(|()| SD::best_priced_fixed_pool(&mut self.fixed_pools[!SD::INPUT_SIDE]))
			.map(|entry| (*entry.key(), entry))
			.filter(|(sqrt_price, _)| {
				sqrt_price_limit.map_or(true, |sqrt_price_limit| {
					!SD::sqrt_price_op_more_than(*sqrt_price, sqrt_price_limit)
				})
			}) {
			let fixed_pool = fixed_pool_entry.get_mut();

			let amount_minus_fees = mul_div_floor(
				amount,
				U256::from(ONE_IN_HUNDREDTH_PIPS - self.fee_hundredth_pips),
				U256::from(ONE_IN_HUNDREDTH_PIPS),
			); /* Will not overflow as fee_hundredth_pips <= ONE_IN_HUNDREDTH_PIPS / 2 */

			let price = sqrt_price_to_price(sqrt_price);
			let amount_required_to_consume_pool =
				SD::input_amount_ceil(fixed_pool.available, price);

			let (output_amount, swapped_amount, fees_taken) = if amount_minus_fees >=
				amount_required_to_consume_pool
			{
				let fixed_pool = fixed_pool_entry.remove();

				let fees = mul_div_ceil(
					amount_required_to_consume_pool,
					U256::from(self.fee_hundredth_pips),
					U256::from(ONE_IN_HUNDREDTH_PIPS - self.fee_hundredth_pips),
				); /* Will not overflow as fee_hundredth_pips <= ONE_IN_HUNDREDTH_PIPS / 2 */

				// Cannot underflow as amount_minus_fees >= amount_required_to_consume_pool
				amount -= amount_required_to_consume_pool + fees;

				(fixed_pool.available, amount_required_to_consume_pool, fees)
			} else {
				let initial_output_amount = SD::output_amount_floor(amount_minus_fees, price);

				// We calculate (output_amount, next_percent_remaining) so that
				// next_percent_remaining is an under-estimate of the remaining liquidity, but also
				// an under-estimate of the used liquidity, by over-estimating it according to
				// used liquidity and then decreasing output_amount so that next_percent_remaining
				// also under-estimates the remaining_liquidity

				let next_percent_remaining = FloatBetweenZeroAndOne::mul_div_ceil(
					&fixed_pool.percent_remaining,
					/* Cannot underflow as amount_required_to_consume_pool is ceiled, but
					 * amount_minus_fees < amount_required_to_consume_pool, therefore
					 * amount_minus_fees <= SD::input_amount_floor(fixed_pool.available, price) */
					fixed_pool.available - initial_output_amount,
					fixed_pool.available,
				);

				// We back-calculate output_amount to ensure output_amount is less than or equal to
				// what percent_remaining suggests has been output
				let output_amount = fixed_pool.available -
					FloatBetweenZeroAndOne::integer_mul_div(
						fixed_pool.available,
						&next_percent_remaining,
						&fixed_pool.percent_remaining,
					)
					.1;

				assert!(output_amount <= initial_output_amount);

				fixed_pool.percent_remaining = next_percent_remaining;
				fixed_pool.available -= output_amount;

				let fees_taken = amount - amount_minus_fees;
				amount = Amount::zero();

				(output_amount, amount_minus_fees, fees_taken)
			};

			self.total_swap_inputs[SD::INPUT_SIDE] =
				self.total_swap_inputs[SD::INPUT_SIDE].saturating_add(swapped_amount);
			self.total_fees_earned[SD::INPUT_SIDE] =
				self.total_fees_earned[SD::INPUT_SIDE].saturating_add(fees_taken);

			total_output_amount = total_output_amount.saturating_add(output_amount);
		}

		self.total_swap_outputs[!SD::INPUT_SIDE] =
			self.total_swap_outputs[!SD::INPUT_SIDE].saturating_add(total_output_amount);

		(total_output_amount, amount)
	}

	fn collect_from_position<SD: SwapDirection>(
		mut position: Position,
		fixed_pool: Option<&FixedPool>,
		price: Price,
		fee_hundredth_pips: u32,
	) -> (Collected, Option<Position>) {
		let previous_position = position.clone();
		let (used_liquidity, option_position) = if let Some(fixed_pool) =
			fixed_pool.filter(|fixed_pool| fixed_pool.pool_instance == position.pool_instance)
		{
			let (remaining_amount_floor, remaining_amount_ceil) =
				FloatBetweenZeroAndOne::integer_mul_div(
					position.amount,
					&fixed_pool.percent_remaining,
					&position.last_percent_remaining,
				);

			(
				// We under-estimate used liquidity so that lp's don't receive more
				// bought_amount and fees than may exist in the pool
				position.amount - remaining_amount_ceil,
				// We under-estimate remaining liquidity so that lp's cannot burn more
				// liquidity than truly exists in the pool
				if remaining_amount_floor.is_zero() {
					None
				} else {
					position.amount = remaining_amount_floor;
					position.last_percent_remaining = fixed_pool.percent_remaining.clone();
					Some(position)
				},
			)
		} else {
			(position.amount, None)
		};

		let bought_amount = SD::input_amount_floor(used_liquidity, price);
		let fees = /* Will not overflow as fee_hundredth_pips <= ONE_IN_HUNDREDTH_PIPS / 2 */ mul_div_floor(
			bought_amount,
			U256::from(fee_hundredth_pips),
			U256::from(ONE_IN_HUNDREDTH_PIPS - fee_hundredth_pips),
		);
		let accumulative_fees = previous_position.accumulative_fees.saturating_add(fees);
		(
			Collected {
				fees,
				bought_amount,
				sold_amount: used_liquidity,
				accumulative_fees,
				original_amount: previous_position.original_amount,
			},
			option_position.map(|mut position| {
				position.accumulative_fees = accumulative_fees;
				position
			}),
		)
	}

	/// Collects any earnings from the specified position, and then adds additional liquidity to it.
	/// The SwapDirection determines which direction of swaps the liquidity/position you're minting
	/// will be for.
	///
	/// This function never panics.
	pub(super) fn collect_and_mint<SD: SwapDirection>(
		&mut self,
		lp: &LiquidityProvider,
		tick: Tick,
		amount: Amount,
	) -> Result<(Collected, PositionInfo), PositionError<MintError>> {
		if amount.is_zero() {
			self.collect::<SD>(lp, tick)
				.map_err(|err| err.map_other(|e| -> MintError { match e {} }))
		} else {
			let sqrt_price = Self::validate_tick(tick)?;
			let price = sqrt_price_to_price(sqrt_price);

			let positions = &mut self.positions[!SD::INPUT_SIDE];
			let fixed_pools = &mut self.fixed_pools[!SD::INPUT_SIDE];

			let option_fixed_pool = fixed_pools.get(&sqrt_price);
			let (collected_amounts, option_position) =
				if let Some(position) = positions.get(&(sqrt_price, lp.clone())).cloned() {
					Self::collect_from_position::<SD>(
						position,
						option_fixed_pool,
						price,
						self.fee_hundredth_pips,
					)
				} else {
					(Default::default(), None)
				};

			let (mut position, mut fixed_pool, next_pool_instance) = if let Some(position) =
				option_position
			{
				(
					position,
					option_fixed_pool.unwrap().clone(), /* Position having liquidity implies
					                                     * fixed pool existing. */
					self.next_pool_instance,
				)
			} else {
				let (fixed_pool, next_pool_instance) = if let Some(fixed_pool) = option_fixed_pool {
					(fixed_pool.clone(), self.next_pool_instance)
				} else {
					(
						FixedPool {
							pool_instance: self.next_pool_instance,
							available: U256::zero(),
							percent_remaining: FloatBetweenZeroAndOne::max(),
						},
						self.next_pool_instance
							.checked_add(1)
							.ok_or(PositionError::Other(MintError::MaximumPoolInstances))?,
					)
				};

				(
					Position {
						pool_instance: fixed_pool.pool_instance,
						amount: Amount::zero(),
						last_percent_remaining: fixed_pool.percent_remaining.clone(),
						accumulative_fees: Amount::zero(),
						original_amount: Amount::zero(),
					},
					fixed_pool,
					next_pool_instance,
				)
			};

			fixed_pool.available = fixed_pool.available.saturating_add(amount);
			if fixed_pool.available > MAX_FIXED_POOL_LIQUIDITY {
				Err(PositionError::Other(MintError::MaximumLiquidity))
			} else {
				position.amount += amount;
				position.original_amount = position.amount;

				let position_info = PositionInfo::from(&position);

				self.next_pool_instance = next_pool_instance;
				fixed_pools.insert(sqrt_price, fixed_pool);
				positions.insert((sqrt_price, lp.clone()), position);

				Ok((collected_amounts, position_info))
			}
		}
	}

	fn validate_tick<T>(tick: Tick) -> Result<SqrtPriceQ64F96, PositionError<T>> {
		is_tick_valid(tick)
			.then(|| sqrt_price_at_tick(tick))
			.ok_or(PositionError::InvalidTick)
	}

	/// Collects any earnings from the specified position, and then removes the requested amount of
	/// liquidity from it. The SwapDirection determines which direction of swaps the
	/// liquidity/position you're burning was for.
	///
	/// This function never panics.
	pub(super) fn collect_and_burn<SD: SwapDirection>(
		&mut self,
		lp: &LiquidityProvider,
		tick: Tick,
		amount: Amount,
	) -> Result<(Amount, Collected, PositionInfo), PositionError<BurnError>> {
		if amount.is_zero() {
			self.collect::<SD>(lp, tick)
				.map_err(|err| err.map_other(|e| -> BurnError { match e {} }))
				.map(|(collected_amounts, position_info)| {
					(Amount::zero(), collected_amounts, position_info)
				})
		} else {
			let sqrt_price = Self::validate_tick(tick)?;
			let price = sqrt_price_to_price(sqrt_price);

			let positions = &mut self.positions[!SD::INPUT_SIDE];
			let fixed_pools = &mut self.fixed_pools[!SD::INPUT_SIDE];

			let position = positions
				.get(&(sqrt_price, lp.clone()))
				.ok_or(PositionError::NonExistent)?
				.clone();
			let option_fixed_pool = fixed_pools.get(&sqrt_price);

			let (collected_amounts, option_position) = Self::collect_from_position::<SD>(
				position,
				option_fixed_pool,
				price,
				self.fee_hundredth_pips,
			);
			Ok(if let Some(mut position) = option_position {
				let mut fixed_pool = option_fixed_pool.unwrap().clone(); // Position having liquidity remaining implies fixed pool existing before collect.

				let amount = core::cmp::min(position.amount, amount);
				position.amount -= amount;
				position.original_amount = position.amount;
				fixed_pool.available -= amount;

				let position_info = PositionInfo::from(&position);

				if position.amount.is_zero() {
					positions.remove(&(sqrt_price, lp.clone()));
				} else {
					assert!(!fixed_pool.available.is_zero());
					positions.insert((sqrt_price, lp.clone()), position);
				};
				if fixed_pool.available.is_zero() {
					fixed_pools.remove(&sqrt_price);
				} else {
					fixed_pools.insert(sqrt_price, fixed_pool);
				}

				(amount, collected_amounts, position_info)
			} else {
				positions.remove(&(sqrt_price, lp.clone()));
				(Default::default(), collected_amounts, PositionInfo::default())
			})
		}
	}

	/// Collects any earnings from the specified position. The SwapDirection determines which
	/// direction of swaps the liquidity/position you're referring to is for.
	///
	/// This function never panics.
	pub(super) fn collect<SD: SwapDirection>(
		&mut self,
		lp: &LiquidityProvider,
		tick: Tick,
	) -> Result<(Collected, PositionInfo), PositionError<CollectError>> {
		let sqrt_price = Self::validate_tick(tick)?;
		self.inner_collect::<SD>(lp, sqrt_price)
	}

	fn inner_collect<SD: SwapDirection>(
		&mut self,
		lp: &LiquidityProvider,
		sqrt_price: SqrtPriceQ64F96,
	) -> Result<(Collected, PositionInfo), PositionError<CollectError>> {
		let price = sqrt_price_to_price(sqrt_price);

		let positions = &mut self.positions[!SD::INPUT_SIDE];
		let fixed_pools = &mut self.fixed_pools[!SD::INPUT_SIDE];

		let (collected_amounts, option_position) = Self::collect_from_position::<SD>(
			positions
				.get(&(sqrt_price, lp.clone()))
				.ok_or(PositionError::NonExistent)?
				.clone(),
			fixed_pools.get(&sqrt_price),
			price,
			self.fee_hundredth_pips,
		);

		Ok((
			collected_amounts,
			if let Some(position) = option_position {
				let position_info = PositionInfo::from(&position);
				positions.insert((sqrt_price, lp.clone()), position);
				position_info
			} else {
				positions.remove(&(sqrt_price, lp.clone()));
				PositionInfo::default()
			},
		))
	}

	/// Returns all the assets associated with a position
	///
	/// This function never panics.
	pub(super) fn position<SD: SwapDirection>(
		&self,
		lp: &LiquidityProvider,
		tick: Tick,
	) -> Result<(Collected, PositionInfo), PositionError<Infallible>> {
		let sqrt_price = Self::validate_tick(tick)?;
		let price = sqrt_price_to_price(sqrt_price);

		let positions = &self.positions[!SD::INPUT_SIDE];
		let fixed_pools = &self.fixed_pools[!SD::INPUT_SIDE];

		let (collected_amounts, option_position) = Self::collect_from_position::<SD>(
			positions
				.get(&(sqrt_price, lp.clone()))
				.ok_or(PositionError::NonExistent)?
				.clone(),
			fixed_pools.get(&sqrt_price),
			price,
			self.fee_hundredth_pips,
		);

		Ok((
			collected_amounts,
			option_position.map_or(Default::default(), |position| PositionInfo::from(&position)),
		))
	}

	/// Returns all the assets available for swaps in a given direction
	///
	/// This function never panics.
	pub(super) fn liquidity<SD: SwapDirection>(&self) -> Vec<(Tick, Amount)> {
		self.fixed_pools[!SD::INPUT_SIDE]
			.iter()
			.map(|(sqrt_price, fixed_pool)| (tick_at_sqrt_price(*sqrt_price), fixed_pool.available))
			.collect()
	}

	/// Returns all the assets available for swaps between two prices (inclusive..exclusive)
	///
	/// This function never panics.
	pub(super) fn depth<SD: SwapDirection>(
		&self,
		range: core::ops::Range<Tick>,
	) -> Result<Amount, DepthError> {
		let start =
			Self::validate_tick::<Infallible>(range.start).map_err(|_| DepthError::InvalidTick)?;
		let end =
			Self::validate_tick::<Infallible>(range.end).map_err(|_| DepthError::InvalidTick)?;
		if start <= end {
			Ok(self.fixed_pools[!SD::INPUT_SIDE]
				.range(start..end)
				.map(|(_, fixed_pool)| fixed_pool.available)
				.fold(Default::default(), |acc, x| acc + x))
		} else {
			Err(DepthError::InvalidTickRange)
		}
	}

	pub fn validate_fees(fee_hundredth_pips: u32) -> bool {
		fee_hundredth_pips <= MAX_LP_FEE
	}
}
