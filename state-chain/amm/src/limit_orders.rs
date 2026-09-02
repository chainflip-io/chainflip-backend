// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

//! This code implements a single liquidity pool pair.
//!
//! This liquidity pool pair allows LPs to specify particular prices
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
//! A swap splits what it takes across the orders at the price it executes at, in proportion to the
//! liquidity each of them provides, and reports the result as a [Fill] per order. The proceeds are
//! owed to the LP there and then, so an order that has been bought in its entirety ceases to
//! exist, and every order the pool holds has liquidity left to sell.

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};
use sp_std::collections::btree_map::{BTreeMap, OccupiedEntry};

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::U256;
use sp_std::vec::Vec;

use crate::common::{BaseToQuote, PoolPairsMap, QuoteToBase};
use cf_amm_math::{is_tick_valid, mul_div_floor_checked, Amount, Price, SqrtPrice, Tick};

/// All the orders selling at a single price. Grouping them is only a way of finding them; the
/// liquidity they provide is whatever they currently hold, it is not tracked separately.
pub(super) type Orders<LiquidityProvider> = BTreeMap<LiquidityProvider, Position>;

pub(super) trait SwapDirection: crate::common::SwapDirection {
	/// Calculates the swap input amount needed to produce an output amount at a price
	fn input_amount_ceil(output: Amount, price: Price) -> Option<Amount>;

	/// Calculates the swap input amount needed to produce an output amount at a price
	fn input_amount_floor(output: Amount, price: Price) -> Option<Amount>;

	/// Calculates the swap output amount produced for an input amount at a price
	fn output_amount_ceil(input: Amount, price: Price) -> Option<Amount>;

	/// Calculates the swap output amount produced for an input amount at a price
	fn output_amount_floor(input: Amount, price: Price) -> Option<Amount>;

	/// Gets the entry for the orders priced best for this direction of swap
	fn best_priced_orders<LP: Ord>(
		orders: &'_ mut BTreeMap<SqrtPrice, Orders<LP>>,
	) -> Option<OccupiedEntry<'_, SqrtPrice, Orders<LP>>>;
}
impl SwapDirection for BaseToQuote {
	fn input_amount_ceil(output: Amount, price: Price) -> Option<Amount> {
		price.input_amount_ceil(output)
	}

	fn input_amount_floor(output: Amount, price: Price) -> Option<Amount> {
		price.input_amount_floor(output)
	}

	fn output_amount_ceil(input: Amount, price: Price) -> Option<Amount> {
		price.output_amount_ceil(input)
	}

	fn output_amount_floor(input: Amount, price: Price) -> Option<Amount> {
		price.output_amount_floor(input)
	}

	fn best_priced_orders<LP: Ord>(
		orders: &'_ mut BTreeMap<SqrtPrice, Orders<LP>>,
	) -> Option<OccupiedEntry<'_, SqrtPrice, Orders<LP>>> {
		orders.last_entry()
	}
}
impl SwapDirection for QuoteToBase {
	fn input_amount_ceil(output: Amount, price: Price) -> Option<Amount> {
		BaseToQuote::output_amount_ceil(output, price)
	}

	fn input_amount_floor(output: Amount, price: Price) -> Option<Amount> {
		BaseToQuote::output_amount_floor(output, price)
	}

	fn output_amount_ceil(input: Amount, price: Price) -> Option<Amount> {
		BaseToQuote::input_amount_ceil(input, price)
	}

	fn output_amount_floor(input: Amount, price: Price) -> Option<Amount> {
		BaseToQuote::input_amount_floor(input, price)
	}

	fn best_priced_orders<LP: Ord>(
		orders: &'_ mut BTreeMap<SqrtPrice, Orders<LP>>,
	) -> Option<OccupiedEntry<'_, SqrtPrice, Orders<LP>>> {
		orders.first_entry()
	}
}

#[derive(Debug)]
pub enum DepthError {
	/// Invalid Price
	InvalidTick,
	/// Start tick must be less than or equal to the end tick
	InvalidTickRange,
}

#[derive(Debug)]
pub enum PositionError {
	/// Invalid Price
	InvalidTick,
	/// Position referenced does not exist
	NonExistent,
}

/// A swap buying into a single limit order. The proceeds are owed to the LP as of the swap, the
/// pool does not hold on to them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fill<LiquidityProvider> {
	pub lp: LiquidityProvider,
	pub tick: Tick,
	/// The amount of the order's liquidity that the swap bought.
	pub sold_amount: Amount,
	/// The proceeds owed to the lp in exchange for it.
	pub bought_amount: Amount,
	/// The liquidity left in the order afterwards. Zero means the order was filled in its
	/// entirety and no longer exists.
	pub remaining_amount: Amount,
}

#[derive(
	Default, Debug, PartialEq, Eq, TypeInfo, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen,
)]
pub struct PositionInfo {
	/// The amount of liquidity in the position after the operation.
	pub amount: Amount,
	/// The amount of liquidity in the position as of the last non-zero mint or burn.
	pub original_amount: Amount,
}
impl PositionInfo {
	pub fn new(amount: Amount) -> Self {
		Self { amount, original_amount: amount }
	}
}
impl<'a> From<&'a Position> for PositionInfo {
	fn from(value: &'a Position) -> Self {
		Self { amount: value.amount, original_amount: value.original_amount }
	}
}

/// Represents a single LP position, i.e. a limit order with liquidity left to sell.
#[derive(
	Clone,
	Debug,
	Default,
	TypeInfo,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	Serialize,
	Deserialize,
	PartialEq,
)]
pub struct Position {
	/// The amount of liquidity provided by this position that has not been bought yet. Never
	/// zero, a position with nothing left to sell is removed from the pool.
	amount: Amount,
	/// This is the original amount of liquidity provider by this position as of its creation. This
	/// value is updated if a non-zero mint or burn is performed on the position.
	original_amount: Amount,
}

#[derive(
	Clone, Debug, TypeInfo, Encode, Decode, DecodeWithMemTracking, Serialize, Deserialize, PartialEq,
)]
pub struct PoolState<LiquidityProvider: Ord> {
	/// Every order with liquidity left to sell, grouped by the asset it is selling and then by the
	/// price it is selling at. Orders are dropped as soon as they are bought in their entirety and
	/// a price with no orders left is removed, so the first and last prices are always the best a
	/// swap can execute at.
	orders: PoolPairsMap<BTreeMap<SqrtPrice, Orders<LiquidityProvider>>>,
	/// Total of all swap inputs over all time (not including fees)
	pub(super) total_swap_inputs: PoolPairsMap<Amount>,
	/// Total of all swap outputs over all time
	total_swap_outputs: PoolPairsMap<Amount>,
}

impl<LiquidityProvider: Clone + Ord> PoolState<LiquidityProvider> {
	/// Creates a new pool state. The pool is created with no liquidity.
	///
	/// This function never panics.
	pub(super) fn new() -> Self {
		Self {
			orders: Default::default(),
			total_swap_inputs: Default::default(),
			total_swap_outputs: Default::default(),
		}
	}

	/// Creates an iterator over all positions
	///
	/// This function never panics.
	pub(super) fn positions<SD: SwapDirection>(
		&self,
	) -> impl '_ + Iterator<Item = (LiquidityProvider, Tick, PositionInfo)> {
		self.orders[!SD::INPUT_SIDE].iter().flat_map(|(sqrt_price, orders)| {
			orders.iter().map(move |(lp, position)| {
				(lp.clone(), sqrt_price.to_tick(), PositionInfo::from(position))
			})
		})
	}

	/// Returns the current price of the pool for a given swap direction, if some liquidity exists.
	///
	/// This function never panics.
	pub(super) fn current_sqrt_price<SD: SwapDirection>(&mut self) -> Option<SqrtPrice> {
		SD::best_priced_orders(&mut self.orders[!SD::INPUT_SIDE]).map(|entry| *entry.key())
	}

	/// Swaps the specified Amount into the other currency until sqrt_price_limit is reached (If
	/// Some), and returns the resulting Amount, the remaining input Amount, and the orders that
	/// were bought into. The direction of the swap is controlled by the generic type parameter
	/// `SD`, by setting it to `BaseToQuote` or `QuoteToBase`. Note sqrt_price_limit is inclusive.
	///
	/// This function never panics
	pub(super) fn swap<SD: SwapDirection>(
		&mut self,
		mut amount: Amount,
		sqrt_price_limit: Option<SqrtPrice>,
		range_orders_pool_fee_hundredth_pips: u32,
	) -> (Amount, Amount, Vec<Fill<LiquidityProvider>>) {
		let mut total_output_amount = U256::zero();
		let mut fills = Vec::new();

		while let Some((sqrt_price, mut orders_entry)) = (!amount.is_zero())
			.then_some(())
			.and_then(|()| SD::best_priced_orders(&mut self.orders[!SD::INPUT_SIDE]))
			.map(|entry| (*entry.key(), entry))
			.filter(|(sqrt_price, _)| {
				// Compare in the clamped `SqrtPrice` domain, consistently with
				// `super::PoolState::inner_swap`.
				let sqrt_price_adjusted = super::sqrt_price_adjusted_by_pool_fee::<SD::Inverse>(
					*sqrt_price,
					range_orders_pool_fee_hundredth_pips,
				);

				sqrt_price_limit.is_none_or(|sqrt_price_limit| {
					!SD::sqrt_price_op_more_than(sqrt_price_adjusted, sqrt_price_limit)
				})
			}) {
			let orders = orders_entry.get_mut();
			let available = liquidity_of(orders);

			let price = Price::from(sqrt_price);

			// Either the input buys all the liquidity at this price, and the swap moves on to the
			// next best price, or the input runs out here and buys as much as it can.
			let (sold_amount, bought_amount) = match SD::input_amount_ceil(available, price) {
				Some(required_to_consume) if amount >= required_to_consume =>
					(available, required_to_consume),
				// An overflowing output would exceed `available`, which bounds it regardless.
				_ => (
					core::cmp::min(
						SD::output_amount_floor(amount, price).unwrap_or(available),
						available,
					),
					amount,
				),
			};

			// Cannot underflow as swapped_amount is bounded by amount in both cases above.
			amount -= bought_amount;

			fill_orders(orders, sqrt_price, available, sold_amount, bought_amount, &mut fills);

			if orders.is_empty() {
				orders_entry.remove();
			}

			self.total_swap_inputs[SD::INPUT_SIDE] =
				self.total_swap_inputs[SD::INPUT_SIDE].saturating_add(bought_amount);

			total_output_amount = total_output_amount.saturating_add(sold_amount);
		}

		self.total_swap_outputs[!SD::INPUT_SIDE] =
			self.total_swap_outputs[!SD::INPUT_SIDE].saturating_add(total_output_amount);

		(total_output_amount, amount, fills)
	}

	/// Adds liquidity to the position at the given tick, creating it if it doesn't exist yet. The
	/// SwapDirection determines which direction of swaps the liquidity/position you're minting
	/// will be for.
	///
	/// This function never panics.
	pub(super) fn mint<SD: SwapDirection>(
		&mut self,
		lp: &LiquidityProvider,
		tick: Tick,
		amount: Amount,
	) -> Result<PositionInfo, PositionError> {
		let sqrt_price = Self::validate_tick(tick)?;
		let orders = &mut self.orders[!SD::INPUT_SIDE];

		if amount.is_zero() {
			return orders
				.get(&sqrt_price)
				.and_then(|orders| orders.get(lp))
				.map(PositionInfo::from)
				.ok_or(PositionError::NonExistent)
		}

		let position = orders.entry(sqrt_price).or_default().entry(lp.clone()).or_default();
		position.amount = position.amount.saturating_add(amount);
		position.original_amount = position.amount;

		Ok(PositionInfo::from(&*position))
	}

	fn validate_tick(tick: Tick) -> Result<SqrtPrice, PositionError> {
		is_tick_valid(tick)
			.then(|| SqrtPrice::from_tick(tick))
			.ok_or(PositionError::InvalidTick)
	}

	/// Removes the requested amount of liquidity from the position at the given tick, returning
	/// the amount actually removed. The SwapDirection determines which direction of swaps the
	/// liquidity/position you're burning was for.
	///
	/// This function never panics.
	pub(super) fn burn<SD: SwapDirection>(
		&mut self,
		lp: &LiquidityProvider,
		tick: Tick,
		amount: Amount,
	) -> Result<(Amount, PositionInfo), PositionError> {
		let sqrt_price = Self::validate_tick(tick)?;

		let all_orders = &mut self.orders[!SD::INPUT_SIDE];
		let orders = all_orders.get_mut(&sqrt_price).ok_or(PositionError::NonExistent)?;
		let position = orders.get_mut(lp).ok_or(PositionError::NonExistent)?;
		if amount.is_zero() {
			return Ok((Amount::zero(), PositionInfo::from(&*position)))
		}

		let burnt_amount = core::cmp::min(position.amount, amount);
		// Cannot underflow, the burn is capped by the position's liquidity.
		position.amount -= burnt_amount;
		position.original_amount = position.amount;
		let position_info = PositionInfo::from(&*position);

		if position.amount.is_zero() {
			orders.remove(lp);
			if orders.is_empty() {
				all_orders.remove(&sqrt_price);
			}
		}

		Ok((burnt_amount, position_info))
	}

	/// Returns the position for the given lp at the given tick.
	///
	/// This function never panics.
	pub(super) fn position<SD: SwapDirection>(
		&self,
		lp: &LiquidityProvider,
		tick: Tick,
	) -> Result<PositionInfo, PositionError> {
		let sqrt_price = Self::validate_tick(tick)?;

		self.orders[!SD::INPUT_SIDE]
			.get(&sqrt_price)
			.and_then(|orders| orders.get(lp))
			.map(PositionInfo::from)
			.ok_or(PositionError::NonExistent)
	}

	/// Returns all the assets available for swaps in a given direction, grouped by tick.
	///
	/// This function never panics.
	pub(super) fn liquidity<SD: SwapDirection>(&self) -> Vec<(Tick, Amount)> {
		self.orders[!SD::INPUT_SIDE]
			.iter()
			.map(|(sqrt_price, orders)| (sqrt_price.to_tick(), liquidity_of(orders)))
			.collect()
	}

	/// Returns all the assets available for swaps between two prices (inclusive..exclusive)
	///
	/// This function never panics.
	pub(super) fn depth<SD: SwapDirection>(
		&self,
		range: core::ops::Range<Tick>,
	) -> Result<Amount, DepthError> {
		let start = Self::validate_tick(range.start).map_err(|_| DepthError::InvalidTick)?;
		let end = Self::validate_tick(range.end).map_err(|_| DepthError::InvalidTick)?;
		if start <= end {
			Ok(self.orders[!SD::INPUT_SIDE]
				.range(start..end)
				.map(|(_, orders)| liquidity_of(orders))
				.fold(Amount::zero(), |total, amount| total.saturating_add(amount)))
		} else {
			Err(DepthError::InvalidTickRange)
		}
	}
}

/// The total liquidity a set of orders provides.
fn liquidity_of<LiquidityProvider: Ord>(orders: &Orders<LiquidityProvider>) -> Amount {
	orders
		.values()
		.fold(Amount::zero(), |total, position| total.saturating_add(position.amount))
}

/// Buys `sold_amount` of the orders' liquidity for `bought_amount` of the other asset, splitting
/// both across them in proportion to the liquidity each of them provides, and appending the result
/// to `fills`. Orders left with nothing to sell are removed.
///
/// `available` must be the total liquidity the orders provide, and `sold_amount` no more than it.
fn fill_orders<LiquidityProvider: Ord + Clone>(
	orders: &mut Orders<LiquidityProvider>,
	sqrt_price: SqrtPrice,
	available: Amount,
	sold_amount: Amount,
	bought_amount: Amount,
	fills: &mut Vec<Fill<LiquidityProvider>>,
) {
	debug_assert!(sold_amount <= available);

	let tick = sqrt_price.to_tick();
	let mut remaining_available = available;
	let mut remaining_sold = sold_amount;
	let mut remaining_bought = bought_amount;

	orders.retain(|lp, position| {
		// Calculate share against remaining amount instead of total amount to avoid rounding
		// errors. The divisions cannot fail: every order holds a non-zero amount, so the
		// remaining liquidity is non-zero for as long as there is an order left to fill.
		let sold = mul_div_floor_checked(remaining_sold, position.amount, remaining_available)
			.unwrap_or(remaining_sold);
		let bought = mul_div_floor_checked(remaining_bought, position.amount, remaining_available)
			.unwrap_or(remaining_bought);

		// Cannot underflow: `remaining_sold` never exceeds `remaining_available`, which bounds
		// an order's share of it by the liquidity that order provides.
		debug_assert!(sold <= position.amount);
		remaining_available = remaining_available.saturating_sub(position.amount);
		if !sold.is_zero() || !bought.is_zero() {
			remaining_sold = remaining_sold.saturating_sub(sold);
			remaining_bought = remaining_bought.saturating_sub(bought);

			position.amount = position.amount.saturating_sub(sold);

			fills.push(Fill {
				lp: lp.clone(),
				tick,
				sold_amount: sold,
				bought_amount: bought,
				remaining_amount: position.amount,
			});
		}

		// An order with nothing left to sell is dropped; its proceeds are owed to the lp, not held
		// by the pool.
		!position.amount.is_zero()
	});
}

/// Decoding and conversion of the limit order state as it was before fixed pools were removed.
///
/// The old representation did not store what an order had earned. It stored the running product of
/// `1 - percent_used` for each price (`percent_remaining`), and each order stored that product as
/// of the last time it was touched; the ratio between the two recovered how much of the order had
/// been bought since. There is nowhere to put that in the new representation, so converting has to
/// realise it: whatever each order had earned and not yet collected comes back as
/// [migration_support::UncollectedProceeds] for the caller to pay out.
///
/// This lives here rather than beside the migration because converting has to construct the new
/// [PoolState] and [Position], whose fields are private to this module. Exposing constructors for
/// them would widen this crate's API permanently for a need that expires.
///
/// It exists solely for `pallet-cf-pools`'s `remove_fixed_pools` migration, and should be deleted
/// with it. The equivalent support for the v7 to v8 migration outlived its migration by several
/// releases because the two sit in different crates.
pub mod migration_support {
	use super::*;
	use crate::common::Pairs;
	use sp_core::U512;

	/// A number in `0.0..1.0`, as the old representation stored it: a normalised mantissa, so the
	/// precision does not decay as the value shrinks, and a 256 bit negative exponent, so the
	/// running product could shrink across effectively unbounded swaps.
	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
	pub struct FloatBetweenZeroAndOne {
		normalised_mantissa: U256,
		negative_exponent: U256,
	}

	impl FloatBetweenZeroAndOne {
		/// Right shifts `x` by `shift_bits`, returning the result and the bits shifted out.
		fn right_shift_mod(x: U512, shift_bits: U256) -> (U512, U512) {
			if shift_bits >= U256::from(512) {
				(U512::zero(), x)
			} else {
				let shift_bits = shift_bits.as_u32();
				(x >> shift_bits, x & (U512::MAX >> (512 - shift_bits)))
			}
		}

		/// The floor and ceil of `x * numerator / denominator`.
		///
		/// Returns `None` rather than panicking on input the old representation could not have
		/// produced, so that a corrupt entry cannot take the chain down mid-migration.
		fn integer_mul_div(x: U256, numerator: &Self, denominator: &Self) -> Option<(U256, U256)> {
			if !numerator.normalised_mantissa.bit(255) || !denominator.normalised_mantissa.bit(255)
			{
				return None
			}

			let (y_shifted_floor, div_remainder) = U512::div_mod(
				U256::full_mul(x, numerator.normalised_mantissa),
				denominator.normalised_mantissa.into(),
			);

			// The numerator must be the smaller number: a price cannot have more left than when
			// the order recorded its share of it. Because both mantissas are normalised, the
			// exponents settle it unless they are equal, so the subtraction catches every case
			// but that one.
			let negative_exponent =
				numerator.negative_exponent.checked_sub(denominator.negative_exponent)?;
			if negative_exponent.is_zero() &&
				numerator.normalised_mantissa > denominator.normalised_mantissa
			{
				return None
			}

			let (y_floor, shift_remainder) =
				Self::right_shift_mod(y_shifted_floor, negative_exponent);

			// Cannot exceed `x` now that the numerator is known to be the smaller of the two, but
			// the conversion stays checked rather than resting on that.
			let y_floor = U256::try_from(y_floor).ok()?;

			Some((
				y_floor,
				if div_remainder.is_zero() && shift_remainder.is_zero() {
					y_floor
				} else {
					// Safe: for there to be a remainder, y_floor is at least one below x.
					y_floor.saturating_add(U256::one())
				},
			))
		}
	}

	/// An order as it stood before the conversion.
	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
	pub struct OrderBefore<LiquidityProvider> {
		/// The price the order was selling at.
		pub tick: Tick,
		pub lp: LiquidityProvider,
		/// What it had left to sell.
		pub amount: Amount,
		/// Its size as of its last update.
		pub original_amount: Amount,
	}

	/// All the liquidity offered at one price, as a single pool.
	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
	pub struct FixedPool {
		pool_instance: u128,
		available: Amount,
		percent_remaining: FloatBetweenZeroAndOne,
	}

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
	pub struct PositionV9 {
		pool_instance: u128,
		amount: Amount,
		last_percent_remaining: FloatBetweenZeroAndOne,
		original_amount: Amount,
	}

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
	pub struct PoolStateV9<LiquidityProvider: Ord> {
		next_pool_instance: u128,
		fixed_pools: PoolPairsMap<BTreeMap<SqrtPrice, FixedPool>>,
		positions: PoolPairsMap<BTreeMap<(SqrtPrice, LiquidityProvider), PositionV9>>,
		total_swap_inputs: PoolPairsMap<Amount>,
		total_swap_outputs: PoolPairsMap<Amount>,
	}

	/// The outcome of converting to the current representation.
	pub struct Migrated<LiquidityProvider: Ord> {
		pub pool_state: PoolState<LiquidityProvider>,
		/// Earnings that have to be paid out, because the new representation cannot hold them.
		pub proceeds: Vec<UncollectedProceeds<LiquidityProvider>>,
		/// Liquidity the fixed pools were offering over and above the orders backing them, by the
		/// pair it was being sold in. Rounding in the old representation let the two drift apart,
		/// and no order could ever claim the difference, so it is dropped.
		pub dropped_dust: PoolPairsMap<Amount>,
		/// Orders dropped because their share of their price would not convert. Expected to be
		/// empty; anything here is an lp owed an amount the migration could not work out,
		/// reported so it can be settled by hand.
		pub unconvertible: Vec<UnconvertibleOrder<LiquidityProvider>>,
	}

	/// An order whose recorded share of its price would not convert, which the old representation
	/// could not produce. The migration drops such an order rather than carrying it over, so this
	/// is everything needed to work out by hand what its lp is owed.
	#[derive(Clone, Debug, PartialEq, Eq)]
	pub struct UnconvertibleOrder<LiquidityProvider> {
		pub lp: LiquidityProvider,
		/// The pair the order was selling. Anything it earned would be in the other one.
		pub sold_pair: Pairs,
		pub tick: Tick,
		/// The size the order last recorded. An upper bound on what its lp is owed, since some
		/// of it may already have been bought.
		pub amount: Amount,
		pub original_amount: Amount,
		/// The order's snapshot of the fraction of its fixed pool still unsold, taken when the
		/// order was last updated. Not a price: the price is `tick`.
		pub order_percent_remaining: FloatBetweenZeroAndOne,
		/// The fraction of that fixed pool still unsold now. What the order had left is `amount`
		/// times this over the snapshot; the rest had been bought.
		pub pool_percent_remaining: Option<FloatBetweenZeroAndOne>,
	}

	/// What an order had earned but not collected. The new representation cannot hold it, so it
	/// has to be paid out as part of the migration.
	#[derive(Clone, Debug, PartialEq, Eq)]
	pub struct UncollectedProceeds<LiquidityProvider> {
		pub lp: LiquidityProvider,
		/// The pair the order was selling. The proceeds are in the other one.
		pub sold_pair: Pairs,
		/// The amount of the order's liquidity that had been bought. Needed to value the fill in
		/// usd terms when the proceeds are not the stable asset.
		pub sold_amount: Amount,
		/// What it earned, owed to the lp.
		pub bought_amount: Amount,
	}

	impl<LiquidityProvider: Clone + Ord> PoolStateV9<LiquidityProvider> {
		/// Converts to the current representation, returning the earnings that have to be paid out
		/// because there is no longer anywhere to keep them.
		pub fn migrate(self) -> Migrated<LiquidityProvider> {
			let Self { fixed_pools, positions, total_swap_inputs, total_swap_outputs, .. } = self;

			let mut proceeds = Vec::new();
			let mut unconvertible = Vec::new();
			let mut orders =
				PoolPairsMap::<BTreeMap<SqrtPrice, Orders<LiquidityProvider>>>::default();

			for sold_pair in [Pairs::Base, Pairs::Quote] {
				for ((sqrt_price, lp), position) in &positions[sold_pair] {
					let fixed_pool = fixed_pools[sold_pair].get(sqrt_price);
					let (remaining_amount, used_amount) =
						match remaining_and_used(position, fixed_pool) {
							Some(converted) => converted,
							None => {
								// Dropped rather than carried over: the size it records is stale,
								// and nothing checks it once the fixed pool is gone, so an order
								// kept here could sell liquidity that had already been bought.
								// Erring towards the lp being owed is recoverable; liquidity
								// conjured out of nothing is not. Reported rather than fatal,
								// because a panic would stop the chain producing blocks.
								unconvertible.push(UnconvertibleOrder {
									lp: lp.clone(),
									sold_pair,
									tick: sqrt_price.to_tick(),
									amount: position.amount,
									original_amount: position.original_amount,
									order_percent_remaining: position
										.last_percent_remaining
										.clone(),
									pool_percent_remaining: fixed_pool
										.map(|fixed_pool| fixed_pool.percent_remaining.clone()),
								});
								(Amount::zero(), Amount::zero())
							},
						};

					if !used_amount.is_zero() {
						// Mirrors what a collect would have paid out at this price.
						let price = Price::from(*sqrt_price);
						let bought_amount = match sold_pair {
							Pairs::Base => price.output_amount_floor(used_amount),
							Pairs::Quote => price.input_amount_floor(used_amount),
						}
						.unwrap_or_default();

						proceeds.push(UncollectedProceeds {
							lp: lp.clone(),
							sold_pair,
							sold_amount: used_amount,
							bought_amount,
						});
					}

					// Build the new list of orders, leaving out the empty ones.
					if !remaining_amount.is_zero() {
						orders[sold_pair].entry(*sqrt_price).or_default().insert(
							lp.clone(),
							Position {
								amount: remaining_amount,
								original_amount: position.original_amount,
							},
						);
					}
				}
			}

			// After collection, because of the rounding down when paying out positions, some dust
			// can be left. We calculate that here just for accounting purposes. It will be
			// dropped.
			let mut dropped_dust = PoolPairsMap::<Amount>::default();
			for sold_pair in [Pairs::Base, Pairs::Quote] {
				for (sqrt_price, fixed_pool) in &fixed_pools[sold_pair] {
					let claimed =
						orders[sold_pair].get(sqrt_price).map_or(Amount::zero(), liquidity_of);
					dropped_dust[sold_pair] = dropped_dust[sold_pair]
						.saturating_add(fixed_pool.available.saturating_sub(claimed));
				}
			}

			Migrated {
				pool_state: PoolState { orders, total_swap_inputs, total_swap_outputs },
				proceeds,
				dropped_dust,
				unconvertible,
			}
		}

		/// The liquidity on offer at each price, by the pair being sold. A price's orders can
		/// never survive the conversion totalling more than the price was offering.
		pub fn available_by_price(&self) -> PoolPairsMap<Vec<(Tick, Amount)>> {
			PoolPairsMap::from_array([Pairs::Base, Pairs::Quote].map(|sold_pair| {
				self.fixed_pools[sold_pair]
					.iter()
					.map(|(sqrt_price, fixed_pool)| (sqrt_price.to_tick(), fixed_pool.available))
					.collect()
			}))
		}

		/// Every order, by the pair it sells. Conversion may shrink an order or remove it, but
		/// never grow one or invent one.
		pub fn orders(&self) -> PoolPairsMap<Vec<OrderBefore<LiquidityProvider>>> {
			PoolPairsMap::from_array([Pairs::Base, Pairs::Quote].map(|sold_pair| {
				self.positions[sold_pair]
					.iter()
					.map(|((sqrt_price, lp), position)| OrderBefore {
						tick: sqrt_price.to_tick(),
						lp: lp.clone(),
						amount: position.amount,
						original_amount: position.original_amount,
					})
					.collect()
			}))
		}

		/// The liquidity the fixed pools are offering, by the pair being sold.
		pub fn total_available(&self) -> PoolPairsMap<Amount> {
			let mut totals = PoolPairsMap::<Amount>::default();
			for sold_pair in [Pairs::Base, Pairs::Quote] {
				for fixed_pool in self.fixed_pools[sold_pair].values() {
					totals[sold_pair] = totals[sold_pair].saturating_add(fixed_pool.available);
				}
			}
			totals
		}
	}

	#[cfg(test)]
	mod float_tests {
		use super::*;

		/// The largest representable value, just under one. An untouched price held this.
		fn one() -> FloatBetweenZeroAndOne {
			FloatBetweenZeroAndOne::one()
		}

		/// `one()` halved `halvings` times.
		fn halved(halvings: u32) -> FloatBetweenZeroAndOne {
			FloatBetweenZeroAndOne::from_parts(U256::MAX, halvings.into())
		}

		#[test]
		fn right_shift_mod_boundaries() {
			use FloatBetweenZeroAndOne as F;

			// A shift of zero is the case conversion hits most often, since an untouched order at
			// an untouched price leaves the exponents equal. It relies on `U512::MAX >> 512`
			// saturating to zero rather than panicking, so it is worth pinning.
			assert_eq!(F::right_shift_mod(U512::MAX, U256::zero()), (U512::MAX, U512::zero()));
			assert_eq!(
				F::right_shift_mod(U512::MAX, 128.into()),
				(U512::MAX >> 128, (U512::one() << 128) - 1)
			);
			assert_eq!(
				F::right_shift_mod(U512::MAX, 255.into()),
				(U512::MAX >> 255, (U256::MAX >> 1).into())
			);
			assert_eq!(
				F::right_shift_mod(U512::MAX, 256.into()),
				(U256::MAX.into(), U256::MAX.into())
			);
			assert_eq!(F::right_shift_mod(U512::MAX, 511.into()), (U512::one(), U512::MAX >> 1));

			// At or past the width everything shifts out, and nothing is lost to a `512 - shift`
			// that would otherwise underflow.
			assert_eq!(F::right_shift_mod(U512::MAX, 512.into()), (U512::zero(), U512::MAX));
			assert_eq!(F::right_shift_mod(U512::MAX, U256::MAX), (U512::zero(), U512::MAX));

			for shift in [U256::zero(), 128.into(), 255.into(), 256.into(), 512.into(), U256::MAX] {
				assert_eq!(
					F::right_shift_mod(U512::zero(), shift),
					(U512::zero(), U512::zero()),
					"nothing to shift out of zero at {shift}"
				);
			}
		}

		#[test]
		fn a_float_divided_by_itself_leaves_the_amount_untouched() {
			for float in [one(), halved(1), halved(255)] {
				for x in [U256::zero(), U256::one(), 1000.into(), U256::MAX] {
					// An order at a price nothing has been bought from keeps all of it
					assert_eq!(
						FloatBetweenZeroAndOne::integer_mul_div(x, &float, &float),
						Some((x, x)),
					);
				}
			}
		}

		#[test]
		fn halving_splits_the_amount_and_reports_both_roundings() {
			// Exactly divisible: floor and ceil agree.
			assert_eq!(
				FloatBetweenZeroAndOne::integer_mul_div(1000.into(), &halved(1), &one()),
				Some((500.into(), 500.into()))
			);
			// Odd, so the two differ by one.
			assert_eq!(
				FloatBetweenZeroAndOne::integer_mul_div(1001.into(), &halved(1), &one()),
				Some((500.into(), 501.into()))
			);
			// A quarter left.
			assert_eq!(
				FloatBetweenZeroAndOne::integer_mul_div(1000.into(), &halved(2), &one()),
				Some((250.into(), 250.into()))
			);
		}

		#[test]
		fn shrinking_far_enough_leaves_nothing() {
			// The ceil still reports one, so the amount counts as bought rather than vanishing.
			assert_eq!(
				FloatBetweenZeroAndOne::integer_mul_div(1000.into(), &halved(255), &one()),
				Some((U256::zero(), U256::one()))
			);
		}

		/// Input the old representation could not have produced. Conversion has to refuse it
		/// rather than panic, so that one corrupt entry cannot halt the chain mid-migration.
		#[test]
		fn malformed_input_is_refused_rather_than_panicking() {
			let denormalised = FloatBetweenZeroAndOne::from_parts(U256::one(), U256::zero());

			assert_eq!(
				FloatBetweenZeroAndOne::integer_mul_div(1000.into(), &denormalised, &one()),
				None,
				"a mantissa without its top bit set is not a float this ever produced"
			);
			assert_eq!(
				FloatBetweenZeroAndOne::integer_mul_div(1000.into(), &one(), &denormalised),
				None
			);

			// A numerator larger than the denominator means a price with *more* left than when
			// the order was minted, which cannot happen: `percent_remaining` only decreases.
			assert_eq!(
				FloatBetweenZeroAndOne::integer_mul_div(1000.into(), &one(), &halved(1)),
				None,
				"caught by the exponent subtraction underflowing"
			);
			assert_eq!(
				FloatBetweenZeroAndOne::integer_mul_div(
					1000.into(),
					&FloatBetweenZeroAndOne::from_parts(U256::MAX, U256::one()),
					&FloatBetweenZeroAndOne::from_parts(U256::MAX / 2 + U256::one(), U256::one()),
				),
				None,
				"equal exponents slip past the subtraction, so the mantissas decide"
			);
			// One bit apart, which is as close to one as a ratio above it can be. Rounding would
			// hide it, so only the mantissa comparison rejects this one.
			assert_eq!(
				FloatBetweenZeroAndOne::integer_mul_div(
					Amount::one(),
					&FloatBetweenZeroAndOne::from_parts(
						(U256::one() << 255) + U256::one(),
						U256::one()
					),
					&FloatBetweenZeroAndOne::from_parts(U256::one() << 255, U256::one()),
				),
				None
			);
		}

		/// Adjacent mantissas at the same exponent: a ratio as close to one as the representation
		/// can express without being one. Conversion still has to round the position down rather
		/// than hand back more than it held.
		#[test]
		fn a_ratio_barely_below_one_rounds_down() {
			let before = one().mul_div_ceil(U256::one() << 255, U256::MAX);
			let after = before.mul_div_ceil(U256::MAX >> 1, U256::one() << 255);

			// Renormalisation moves the mantissa up even though the value went down, which is what
			// makes this the awkward case.
			assert!(before.normalised_mantissa < after.normalised_mantissa);
			assert_eq!(
				FloatBetweenZeroAndOne::integer_mul_div(U256::MAX, &after, &before),
				Some((U256::MAX - 2, U256::MAX - 1))
			);
		}

		/// A `percent_remaining` on chain is the product of one `mul_div_ceil` per swap that took
		/// liquidity from its price — hundreds of arbitrary ratios, not the round halvings above.
		/// Converting a position through such a float has to land between the floor and the ceil of
		/// the same chain of ratios computed exactly.
		#[test]
		fn conversion_is_bracketed_by_exact_arithmetic() {
			use cf_amm_math::{mul_div_ceil_checked, test_utilities::rng_u256_inclusive_bound};
			use rand::{Rng, SeedableRng};

			fn rng_u256(rng: &mut impl Rng) -> U256 {
				U256([(); 4].map(|()| rng.gen()))
			}

			/// A ratio no greater than one, which is all a price's product was ever folded with.
			fn rng_ratio(rng: &mut impl Rng) -> (U256, U256) {
				let numerator = rng_u256(rng);
				(numerator, rng_u256_inclusive_bound(rng, numerator..=U256::MAX))
			}

			let mut rng = rand::rngs::StdRng::from_seed([8u8; 32]);

			for _ in 0..1024 {
				let minted = rng_u256(&mut rng);

				let (floor, ceil, percent_remaining) = (0..rng.gen_range(8..256))
					.map(|_| rng_ratio(&mut rng))
					.fold((minted, minted, one()), |(floor, ceil, float), (n, d)| {
						(
							mul_div_floor_checked(floor, n, d).unwrap(),
							mul_div_ceil_checked(ceil, n, d).unwrap(),
							float.mul_div_ceil(n, d),
						)
					});

				let (remaining, _) =
					FloatBetweenZeroAndOne::integer_mul_div(minted, &percent_remaining, &one())
						.unwrap();

				assert!(
					floor <= remaining && remaining <= ceil,
					"{remaining} outside {floor}..={ceil}"
				);
			}
		}
	}

	#[cfg(any(test, feature = "test"))]
	mod test_constructors {
		use super::*;

		impl FloatBetweenZeroAndOne {
			pub fn from_parts(normalised_mantissa: U256, negative_exponent: U256) -> Self {
				Self { normalised_mantissa, negative_exponent }
			}

			/// The largest representable value, `1.0 - 2^-256`. A price nothing had been bought
			/// from held exactly this.
			pub fn one() -> Self {
				Self { normalised_mantissa: U256::max_value(), negative_exponent: U256::zero() }
			}

			/// `self * numerator / denominator`, rounded up.
			///
			/// Ported verbatim from the old representation, which folded one of these into a
			/// price's running product for every swap that took liquidity from it. Building
			/// fixtures the same way is what makes them the same shape as the values on chain:
			/// arbitrary reals, not round numbers.
			pub fn mul_div_ceil(&self, numerator: U256, denominator: U256) -> Self {
				assert!(!numerator.is_zero());
				assert!(numerator <= denominator);
				assert!(self.normalised_mantissa.bit(255));

				// The multiply comes first so that precision is not lost off the bottom of the
				// mantissa during the division.
				let (mul_normalised_mantissa, mul_normalise_shift) = {
					let unnormalised_mantissa = U256::full_mul(self.normalised_mantissa, numerator);
					let normalize_shift = unnormalised_mantissa.leading_zeros();
					(unnormalised_mantissa << normalize_shift, 256 - normalize_shift)
				};

				let (mul_div_normalised_mantissa, div_normalise_shift) = {
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

				match self
					.negative_exponent
					.checked_add(U256::from(div_normalise_shift - mul_normalise_shift))
				{
					Some(negative_exponent) =>
						Self { normalised_mantissa: mul_div_normalised_mantissa, negative_exponent },
					// The old representation clamped here rather than overflowing.
					None => Self {
						normalised_mantissa: U256::one() << 255,
						negative_exponent: U256::MAX,
					},
				}
			}
		}

		impl FixedPool {
			pub fn from_parts(
				pool_instance: u128,
				available: Amount,
				percent_remaining: FloatBetweenZeroAndOne,
			) -> Self {
				Self { pool_instance, available, percent_remaining }
			}
		}

		impl PositionV9 {
			pub fn from_parts(
				pool_instance: u128,
				amount: Amount,
				last_percent_remaining: FloatBetweenZeroAndOne,
				original_amount: Amount,
			) -> Self {
				Self { pool_instance, amount, last_percent_remaining, original_amount }
			}
		}

		impl<LiquidityProvider: Ord> PoolStateV9<LiquidityProvider> {
			pub fn from_parts(
				fixed_pools: PoolPairsMap<BTreeMap<SqrtPrice, FixedPool>>,
				positions: PoolPairsMap<BTreeMap<(SqrtPrice, LiquidityProvider), PositionV9>>,
			) -> Self {
				Self {
					next_pool_instance: 0,
					fixed_pools,
					positions,
					total_swap_inputs: Default::default(),
					total_swap_outputs: Default::default(),
				}
			}

			pub fn set_swap_totals(
				&mut self,
				inputs: PoolPairsMap<Amount>,
				outputs: PoolPairsMap<Amount>,
			) {
				self.total_swap_inputs = inputs;
				self.total_swap_outputs = outputs;
			}
		}
	}

	/// How much of an order was left, and how much of it had been bought, given the fixed pool it
	/// belonged to.
	/// `None` where the position's recorded share of the price could not be converted, which the
	/// old representation could not produce: `percent_remaining` only ever decreases and an order
	/// records it at an earlier point.
	fn remaining_and_used(
		position: &PositionV9,
		fixed_pool: Option<&FixedPool>,
	) -> Option<(Amount, Amount)> {
		let Some(fixed_pool) =
			fixed_pool.filter(|fixed_pool| fixed_pool.pool_instance == position.pool_instance)
		else {
			// No fixed pool at this price,  means every last unit of
			// this order was bought.
			return Some((Amount::zero(), position.amount))
		};

		FloatBetweenZeroAndOne::integer_mul_div(
			position.amount,
			&fixed_pool.percent_remaining,
			&position.last_percent_remaining,
		)
		// As when collecting: the remainder is rounded down so an lp cannot withdraw liquidity
		// that isn't there, and the amount used is rounded down so they cannot be paid for more
		// than was bought.
		.map(|(remaining_floor, remaining_ceil)| {
			(remaining_floor, position.amount.saturating_sub(remaining_ceil))
		})
	}
}

#[cfg(test)]
mod migration_tests {
	use super::{migration_support::*, *};
	use crate::common::Pairs;
	use cf_utilities::{assert_matches, assert_ok};

	type Lp = cf_primitives::AccountId;
	type OldPoolState = PoolStateV9<Lp>;

	fn lp(id: u8) -> Lp {
		Lp::from([id; 32])
	}

	/// A price nothing has been bought from.
	fn untouched() -> FloatBetweenZeroAndOne {
		FloatBetweenZeroAndOne::one()
	}

	/// A price that swaps have taken these fractions of, folded into the running product one at a
	/// time exactly as the swaps would have. Real prices are a long chain of these, not a single
	/// ratio, so building fixtures this way gives them the same shape.
	fn remaining_after(
		fractions: impl IntoIterator<Item = (u128, u128)>,
	) -> FloatBetweenZeroAndOne {
		fractions.into_iter().fold(untouched(), |remaining, (numerator, denominator)| {
			remaining.mul_div_ceil(numerator.into(), denominator.into())
		})
	}

	/// Tick zero, so a price of one, which makes the amount bought equal to the amount sold and
	/// keeps the arithmetic under test visible.
	fn at_tick_zero() -> SqrtPrice {
		SqrtPrice::from_tick(0)
	}

	fn old_state(
		fixed_pools: Vec<(SqrtPrice, u128, Amount, FloatBetweenZeroAndOne)>,
		positions: Vec<(SqrtPrice, Lp, u128, Amount, FloatBetweenZeroAndOne, Amount)>,
	) -> OldPoolState {
		OldPoolState::from_parts(
			PoolPairsMap::from_array([
				fixed_pools
					.into_iter()
					.map(|(sqrt_price, instance, available, remaining)| {
						(sqrt_price, FixedPool::from_parts(instance, available, remaining))
					})
					.collect(),
				Default::default(),
			]),
			PoolPairsMap::from_array([
				positions
					.into_iter()
					.map(|(sqrt_price, lp, instance, amount, remaining, original)| {
						(
							(sqrt_price, lp),
							PositionV9::from_parts(instance, amount, remaining, original),
						)
					})
					.collect(),
				Default::default(),
			]),
		)
	}

	#[test]
	fn untouched_orders_survive_intact_and_earn_nothing() {
		let Migrated { pool_state: state, proceeds, .. } = old_state(
			vec![(at_tick_zero(), 0, 1000.into(), untouched())],
			vec![(at_tick_zero(), lp(1), 0, 1000.into(), untouched(), 1000.into())],
		)
		.migrate();

		assert!(proceeds.is_empty(), "nothing has been bought, so nothing is owed");
		assert_eq!(state.liquidity::<QuoteToBase>(), vec![(0, 1000.into())]);
		assert_eq!(
			assert_ok!(state.position::<QuoteToBase>(&lp(1), 0)),
			PositionInfo { amount: 1000.into(), original_amount: 1000.into() }
		);
	}

	#[test]
	fn a_partly_bought_price_splits_the_order() {
		// The price has an eighth of its liquidity left, and both orders were minted when it was
		// full. Uneven sizes, so an lp's remainder, their payout, and the other lp's share are all
		// distinct numbers.
		let Migrated { pool_state: state, proceeds, .. } = old_state(
			vec![(at_tick_zero(), 0, 375.into(), remaining_after([(1, 8)]))],
			vec![
				(at_tick_zero(), lp(1), 0, 1000.into(), untouched(), 1000.into()),
				(at_tick_zero(), lp(2), 0, 2000.into(), untouched(), 2000.into()),
			],
		)
		.migrate();

		// Each order is settled against its own size, rather than one being filled before the
		// next is touched.
		assert_eq!(
			proceeds,
			vec![
				UncollectedProceeds {
					lp: lp(1),
					sold_pair: Pairs::Base,
					sold_amount: 875.into(),
					bought_amount: 875.into(),
				},
				UncollectedProceeds {
					lp: lp(2),
					sold_pair: Pairs::Base,
					sold_amount: 1750.into(),
					bought_amount: 1750.into(),
				},
			]
		);
		assert_eq!(state.liquidity::<QuoteToBase>(), vec![(0, 375.into())]);
		assert_eq!(
			assert_ok!(state.position::<QuoteToBase>(&lp(1), 0)),
			PositionInfo { amount: 125.into(), original_amount: 1000.into() }
		);
		assert_eq!(
			assert_ok!(state.position::<QuoteToBase>(&lp(2), 0)),
			PositionInfo { amount: 250.into(), original_amount: 2000.into() }
		);
	}

	#[test]
	fn orders_bought_in_their_entirety_are_paid_out_and_dropped() {
		// No fixed pool at the price at all: the last of it was bought and the pool deleted.
		let Migrated { pool_state: state, proceeds, .. } = old_state(
			vec![],
			vec![(at_tick_zero(), lp(1), 0, 1000.into(), untouched(), 1000.into())],
		)
		.migrate();

		assert_eq!(
			proceeds,
			vec![UncollectedProceeds {
				lp: lp(1),
				sold_pair: Pairs::Base,
				sold_amount: 1000.into(),
				bought_amount: 1000.into(),
			}]
		);
		assert!(state.liquidity::<QuoteToBase>().is_empty());
		assert_matches!(state.position::<QuoteToBase>(&lp(1), 0), Err(PositionError::NonExistent));
	}

	#[test]
	fn an_order_from_an_earlier_incarnation_of_a_price_is_fully_bought() {
		// Liquidity returned to the price after the pool was emptied, so the pool exists again but
		// under a new instance. The order belongs to the old one and was bought in full.
		let Migrated { pool_state: state, proceeds, .. } = old_state(
			vec![(at_tick_zero(), 7, 400.into(), untouched())],
			vec![
				(at_tick_zero(), lp(1), 0, 1000.into(), untouched(), 1000.into()),
				(at_tick_zero(), lp(2), 7, 400.into(), untouched(), 400.into()),
			],
		)
		.migrate();

		assert_eq!(
			proceeds,
			vec![UncollectedProceeds {
				lp: lp(1),
				sold_pair: Pairs::Base,
				sold_amount: 1000.into(),
				bought_amount: 1000.into(),
			}],
			"only the order from the earlier incarnation is settled"
		);
		// The order from the current incarnation is untouched.
		assert_eq!(state.liquidity::<QuoteToBase>(), vec![(0, 400.into())]);
		assert_eq!(
			assert_ok!(state.position::<QuoteToBase>(&lp(2), 0)),
			PositionInfo { amount: 400.into(), original_amount: 400.into() }
		);
	}

	#[test]
	fn rounding_favours_the_pool_over_the_lp() {
		// An odd amount at a half-bought price: the lp keeps the floor of what is left and is paid
		// for the floor of what went, so neither number is rounded up in their favour.
		let Migrated { pool_state: state, proceeds, .. } = old_state(
			vec![(at_tick_zero(), 0, 501.into(), remaining_after([(1, 2)]))],
			vec![(at_tick_zero(), lp(1), 0, 1001.into(), untouched(), 1001.into())],
		)
		.migrate();

		let remaining = state.liquidity::<QuoteToBase>()[0].1;
		let sold = proceeds[0].sold_amount;

		assert_eq!(remaining, 500.into());
		assert_eq!(sold, 500.into());
		assert!(remaining + sold <= 1001.into(), "the lp must not end up with more than they had");
	}

	/// Rounding in the old representation let a price offer more than the orders behind it backed.
	/// Nobody could claim the difference then either, so it is reported and dropped rather than
	/// handed to whichever order happened to be there.
	#[test]
	fn liquidity_no_order_can_claim_is_dropped() {
		let orphaned_tick = 120;

		let Migrated { pool_state: state, dropped_dust, .. } = old_state(
			vec![
				// Backed by the order below, but offering three units more than it holds.
				(at_tick_zero(), 0, 1003.into(), untouched()),
				// Entirely orphaned: every order that once backed this price is gone.
				// A different price, so a different pool: one counter served them all.
				(SqrtPrice::from_tick(orphaned_tick), 1, 47.into(), untouched()),
			],
			vec![(at_tick_zero(), lp(1), 0, 1000.into(), untouched(), 1000.into())],
		)
		.migrate();

		assert_eq!(dropped_dust[Pairs::Base], (3 + 47).into());
		assert_eq!(dropped_dust[Pairs::Quote], Amount::zero());

		// Only what an order actually held carries over, and the orphaned price is gone.
		assert_eq!(state.liquidity::<QuoteToBase>(), vec![(0, 1000.into())]);
	}

	/// Real prices hold arbitrary fractions of their liquidity, not round ones, and the ratios in
	/// the other tests are all powers of two — the one shape where the arithmetic is cleanest.
	/// Whatever the ratio, an order must be split into no more than it held.
	#[test]
	fn an_order_is_never_split_into_more_than_it_held() {
		let amount = Amount::from(1_000_000u32);

		for remaining @ (numerator, denominator) in
			[(1, 3), (2, 7), (37, 100), (999_999, 1_000_000), (1, 1_000_000)]
		{
			// The swaps that took the price down took the liquidity with them, so what the
			// price still offers is the order's share of it.
			let expected = amount * Amount::from(numerator) / Amount::from(denominator);

			let Migrated { pool_state: state, proceeds, .. } = old_state(
				vec![(at_tick_zero(), 0, expected, remaining_after([remaining]))],
				vec![(at_tick_zero(), lp(1), 0, amount, untouched(), amount)],
			)
			.migrate();

			let kept = state
				.liquidity::<QuoteToBase>()
				.first()
				.map_or(Amount::zero(), |(_tick, amount)| *amount);
			let sold = proceeds.first().map_or(Amount::zero(), |proceeds| proceeds.sold_amount);

			assert!(
				kept + sold <= amount,
				"{numerator}/{denominator} left: kept {kept} and sold {sold}, more than the \
				 {amount} the order held"
			);

			// And the split lands where the ratio says it should, give or take the rounding.
			assert!(
				kept.abs_diff(expected) <= 2.into(),
				"{numerator}/{denominator} left: kept {kept}, expected about {expected}"
			);
		}
	}

	/// A price on chain has been bought down by many swaps, so its running product is a long chain
	/// of multiplications rather than the single ratio every other test here uses. Compounding is
	/// what the float existed to survive, so it is worth checking an order still splits sensibly
	/// after it.
	#[test]
	fn a_price_bought_down_over_many_swaps_still_splits_its_order() {
		let amount = Amount::from(1_000_000_000u32);
		let swaps = [(9, 10), (3, 4), (5, 7), (99, 100), (1, 3), (17, 19), (2, 5)];

		// Roughly what the price still offers after each swap has taken its cut in turn.
		let expected = swaps.iter().fold(amount, |amount, (numerator, denominator)| {
			amount * Amount::from(*numerator) / Amount::from(*denominator)
		});

		let Migrated { pool_state: state, proceeds, .. } = old_state(
			vec![(at_tick_zero(), 0, expected, remaining_after(swaps))],
			vec![(at_tick_zero(), lp(1), 0, amount, untouched(), amount)],
		)
		.migrate();

		let kept = state
			.liquidity::<QuoteToBase>()
			.first()
			.map_or(Amount::zero(), |(_tick, amount)| *amount);
		let sold = proceeds.first().map_or(Amount::zero(), |proceeds| proceeds.sold_amount);

		assert!(kept + sold <= amount, "kept {kept} and sold {sold} of an order holding {amount}");

		assert!(
			kept.abs_diff(expected) <= 8.into(),
			"kept {kept} after {} swaps, expected about {expected}",
			swaps.len()
		);
	}

	#[test]
	fn swap_totals_carry_across() {
		let mut old = old_state(vec![], vec![]);
		old.set_swap_totals(
			PoolPairsMap::from_array([11.into(), 22.into()]),
			PoolPairsMap::from_array([33.into(), 44.into()]),
		);

		let Migrated { pool_state: state, .. } = old.migrate();

		assert_eq!(state.total_swap_inputs, PoolPairsMap::from_array([11.into(), 22.into()]));
		assert_eq!(state.total_swap_outputs, PoolPairsMap::from_array([33.into(), 44.into()]));
	}
}
