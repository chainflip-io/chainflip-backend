#[cfg(test)]
mod tests;

use core::convert::Infallible;

use sp_std::collections::btree_map::BTreeMap;

use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{U256, U512};
use sp_std::vec::Vec;

use crate::common::{
	is_tick_valid, mul_div_ceil, mul_div_floor, sqrt_price_at_tick, sqrt_price_to_price,
	tick_at_sqrt_price, Amount, OneToZero, Price, SideMap, SqrtPriceQ64F96, Tick, ZeroToOne,
	ONE_IN_HUNDREDTH_PIPS, PRICE_FRACTIONAL_BITS,
};

const MAX_FIXED_POOL_LIQUIDITY: Amount = U256([u64::MAX, u64::MAX, 0, 0]);

/// Represents a number exclusively between 0 and 1.
#[derive(Clone, Debug, PartialEq, Eq, TypeInfo, Encode, Decode, MaxEncodedLen)]
#[cfg_attr(feature = "std", derive(Default))]
struct FloatBetweenZeroAndOne {
	normalised_mantissa: U256,
	negative_exponent: U256,
}
impl Ord for FloatBetweenZeroAndOne {
	fn cmp(&self, other: &Self) -> core::cmp::Ordering {
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
	/// Returns the largest possible value.
	fn max() -> Self {
		Self { normalised_mantissa: U256::max_value(), negative_exponent: U256::zero() }
	}

	/// Rights shifts x by shift_bits bits, returning the result and the bits that were shifted
	/// out/remainder.
	fn right_shift_mod(x: U512, shift_bits: U256) -> (U512, U512) {
		if shift_bits >= U256::from(512) {
			(U512::zero(), x)
		} else {
			let shift_bits = shift_bits.as_u32();
			(x >> shift_bits, x & (U512::MAX >> (512 - shift_bits)))
		}
	}

	/// Returns the result of `self * numerator / denominator` with the result rounded up.
	fn mul_div_ceil(&self, numerator: U256, denominator: U256) -> Self {
		// We cannot use the `mul_div_ceil` function here (and then right-shift the result) to
		// calculate the normalised_mantissa as the low zero bits (where we shifted) could be wrong.

		assert!(!numerator.is_zero());
		assert!(numerator <= denominator);
		self.assert_valid();

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
			// 256 bits, so we maintain atleast 256 accurate bits in the result.
			let (d, div_remainder) =
				U512::div_mod(mul_normalised_mantissa, U512::from(denominator));
			let d = if div_remainder.is_zero() { d } else { d + U512::one() };
			let normalise_shift = d.leading_zeros();
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
			// never happen, as atleast (U256::MAX / 256) (~10^74) swaps would have to happen to get
			// into this situation. TODO: A possible solution is disabling minting for pools "close"
			// to this minimum. With a small change to the swapping logic it would be possible to
			// guarantee that the pool would be emptied before percent_remaining could reach this
			// min bound.
			Self { normalised_mantissa: U256::one() << 255, negative_exponent: U256::MAX }
		}
	}

	/// Returns both floor and ceil of `x * numerator / denominator`
	fn integer_mul_div(x: U256, numerator: &Self, denominator: &Self) -> (U256, U256) {
		// Note this does not imply numerator.normalised_mantissa <= denominator.normalised_mantissa
		assert!(numerator <= denominator);
		numerator.assert_valid();
		denominator.assert_valid();

		let (y_shifted_floor, div_remainder) = U512::div_mod(
			U256::full_mul(x, numerator.normalised_mantissa),
			denominator.normalised_mantissa.into(),
		);

		let negative_exponent =
			numerator.negative_exponent.checked_sub(denominator.negative_exponent).unwrap();

		let (y_floor, shift_remainder) = Self::right_shift_mod(y_shifted_floor, negative_exponent);

		let y_floor = y_floor.try_into().unwrap();

		(
			y_floor,
			if div_remainder.is_zero() && shift_remainder.is_zero() {
				y_floor
			} else {
				y_floor + 1
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
impl SwapDirection for ZeroToOne {
	fn input_amount_ceil(output: Amount, price: Price) -> Amount {
		mul_div_ceil(output, U256::one() << PRICE_FRACTIONAL_BITS, price)
	}

	fn input_amount_floor(output: Amount, price: Price) -> Amount {
		OneToZero::output_amount_floor(output, price)
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
impl SwapDirection for OneToZero {
	fn input_amount_ceil(output: Amount, price: Price) -> Amount {
		mul_div_ceil(output, price, U256::one() << PRICE_FRACTIONAL_BITS)
	}

	fn input_amount_floor(output: Amount, price: Price) -> Amount {
		ZeroToOne::output_amount_floor(output, price)
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
pub enum SetFeesError {
	/// Fee must be between 0 - 50%
	InvalidFeeAmount,
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
	pub fees: Amount,
	pub swapped_liquidity: Amount,
}

#[derive(Default, Debug, PartialEq, Eq, TypeInfo, Encode, Decode, MaxEncodedLen)]
pub struct PositionInfo {
	amount: Amount,
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

#[derive(Clone, Debug, TypeInfo, Encode, Decode, MaxEncodedLen)]
struct Position {
	pool_instance: u128,
	amount: Amount,
	last_percent_remaining: FloatBetweenZeroAndOne,
}

#[derive(Clone, Debug, TypeInfo, Encode, Decode, MaxEncodedLen)]
pub(super) struct FixedPool {
	pool_instance: u128,
	available: Amount,
	percent_remaining: FloatBetweenZeroAndOne,
}

#[derive(Clone, Debug, TypeInfo, Encode, Decode)]
pub(super) struct PoolState<LiquidityProvider> {
	fee_hundredth_pips: u32,
	next_pool_instance: u128,
	fixed_pools: SideMap<BTreeMap<SqrtPriceQ64F96, FixedPool>>,
	positions: SideMap<BTreeMap<(SqrtPriceQ64F96, LiquidityProvider), Position>>,
}

impl<LiquidityProvider: Clone + Ord> PoolState<LiquidityProvider> {
	/// Creates a new pool state with the given fee. The pool is created with no liquidity.
	///
	/// This function never panics.
	pub(super) fn new(fee_hundredth_pips: u32) -> Result<Self, NewError> {
		(fee_hundredth_pips <= ONE_IN_HUNDREDTH_PIPS / 2)
			.then_some(())
			.ok_or(NewError::InvalidFeeAmount)?;

		Ok(Self {
			fee_hundredth_pips,
			next_pool_instance: 0,
			fixed_pools: Default::default(),
			positions: Default::default(),
		})
	}

	/// Sets the fee for the pool. This will apply to future swaps. This function will fail if the
	/// fee is greater than 50%. Also runs collect for all positions in the pool.
	///
	/// This function never panics.
	#[allow(clippy::type_complexity)]
	#[allow(dead_code)]
	pub(super) fn set_fees(
		&mut self,
		fee_hundredth_pips: u32,
	) -> Result<
		SideMap<BTreeMap<(SqrtPriceQ64F96, LiquidityProvider), (Collected, PositionInfo)>>,
		SetFeesError,
	> {
		(fee_hundredth_pips <= ONE_IN_HUNDREDTH_PIPS / 2)
			.then_some(())
			.ok_or(SetFeesError::InvalidFeeAmount)?;

		// We must collect all positions before we can change the fee, otherwise the fee and swapped
		// liquidity calculations would be wrong.
		let collected_amounts = [
			self.positions[!<OneToZero as crate::common::SwapDirection>::INPUT_SIDE]
				.keys()
				.cloned()
				.collect::<sp_std::vec::Vec<_>>()
				.into_iter()
				.map(|(sqrt_price, lp)| {
					(
						(sqrt_price, lp.clone()),
						self.inner_collect::<OneToZero>(&lp, sqrt_price).unwrap(),
					)
				})
				.collect(),
			self.positions[!<ZeroToOne as crate::common::SwapDirection>::INPUT_SIDE]
				.keys()
				.cloned()
				.collect::<sp_std::vec::Vec<_>>()
				.into_iter()
				.map(|(sqrt_price, lp)| {
					(
						(sqrt_price, lp.clone()),
						self.inner_collect::<ZeroToOne>(&lp, sqrt_price).unwrap(),
					)
				})
				.collect(),
		];

		self.fee_hundredth_pips = fee_hundredth_pips;

		Ok(SideMap::from_array(collected_amounts))
	}

	/// Returns the current price of the pool, if some liquidity exists.
	///
	/// This function never panics.
	pub(super) fn current_sqrt_price<SD: SwapDirection>(&mut self) -> Option<SqrtPriceQ64F96> {
		SD::best_priced_fixed_pool(&mut self.fixed_pools[!SD::INPUT_SIDE]).map(|entry| *entry.key())
	}

	/// Swaps the specified Amount into the other currency until sqrt_price_limit is reached (If
	/// Some), and returns the resulting Amount and the remaining input Amount. The direction of the
	/// swap is controlled by the generic type parameter `SD`, by setting it to `ZeroToOne` or
	/// `OneToZero`. Note sqrt_price_limit is inclusive.
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

			let output_amount = if amount_minus_fees >= amount_required_to_consume_pool {
				let fixed_pool = fixed_pool_entry.remove();

				// Cannot underflow as amount_minus_fees >= amount_required_to_consume_pool
				amount -= amount_required_to_consume_pool +
					mul_div_ceil(
						amount_required_to_consume_pool,
						U256::from(self.fee_hundredth_pips),
						U256::from(ONE_IN_HUNDREDTH_PIPS - self.fee_hundredth_pips),
					); /* Will not overflow as fee_hundredth_pips <= ONE_IN_HUNDREDTH_PIPS / 2 */

				fixed_pool.available
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
				amount = Amount::zero();

				output_amount
			};

			total_output_amount = total_output_amount.saturating_add(output_amount);
		}

		(total_output_amount, amount)
	}

	fn collect_from_position<SD: SwapDirection>(
		mut position: Position,
		fixed_pool: Option<&FixedPool>,
		price: Price,
		fee_hundredth_pips: u32,
	) -> (Collected, Option<Position>) {
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
				// swapped_liquidity and fees than may exist in the pool
				position.amount - remaining_amount_ceil,
				// We under-estimate remaining liquidity so that lp's cannot burn more liquidity
				// than truely exists in the pool
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

		let swapped_liquidity = SD::input_amount_floor(used_liquidity, price);
		(
			Collected {
				fees: /* Will not overflow as fee_hundredth_pips <= ONE_IN_HUNDREDTH_PIPS / 2 */ mul_div_floor(
					swapped_liquidity,
					U256::from(fee_hundredth_pips),
					U256::from(ONE_IN_HUNDREDTH_PIPS - fee_hundredth_pips),
				),
				swapped_liquidity,
			},
			option_position,
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
						amount: U256::zero(),
						last_percent_remaining: fixed_pool.percent_remaining.clone(),
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
	/// direction of swaps the liquidity/position you're refering to is for.
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
	#[allow(dead_code)]
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
	#[allow(dead_code)]
	pub(super) fn liquidity<SD: SwapDirection>(&self) -> Vec<(Tick, Amount)> {
		self.fixed_pools[!SD::INPUT_SIDE]
			.iter()
			.map(|(sqrt_price, fixed_pool)| (tick_at_sqrt_price(*sqrt_price), fixed_pool.available))
			.collect()
	}
}
