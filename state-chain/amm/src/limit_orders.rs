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

use core::convert::Infallible;

use serde::{Deserialize, Serialize};
use sp_std::collections::btree_map::{BTreeMap, OccupiedEntry};

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::U256;
use sp_std::vec::Vec;

use crate::common::{BaseToQuote, PoolPairsMap, QuoteToBase};
use cf_amm_math::{is_tick_valid, mul_div_floor_checked, Amount, Price, SqrtPrice, Tick};

// This is the maximum liquidity/amount of an asset that can be sold at a single tick/price. If an
// LP attempts to add more liquidity that would increase the total at the tick past this value, the
// minting operation will error. Note this maximum is for all lps combined, and not a single lp,
// therefore it is possible for an LP to "consume" a tick by filling it up to the maximum, and
// thereby not allowing other LPs to mint at that price (But the maximum is high enough that this is
// not feasible).
const MAX_LIQUIDITY_PER_PRICE: Amount = U256([u64::MAX, u64::MAX, 0, 0] /* little endian */);

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
pub enum MintError {
	/// One of the start/end ticks of the range reached its maximum gross liquidity
	MaximumLiquidity,
}

#[derive(Debug)]
pub enum PositionError<T> {
	/// Invalid Price
	InvalidTick,
	/// Position referenced does not exist
	NonExistent,
	Other(T),
}

#[derive(Debug)]
pub enum BurnError {}

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
			let amount_required_to_consume_orders = SD::input_amount_ceil(available, price)
				.expect("Amount and price are assumed to be valid");

			// Either the input buys all the liquidity at this price, and the swap moves on to the
			// next best price, or the input runs out here and buys as much as it can.
			let (sold_amount, bought_amount) = if amount >= amount_required_to_consume_orders {
				(available, amount_required_to_consume_orders)
			} else {
				(
					core::cmp::min(
						SD::output_amount_floor(amount, price)
							.expect("Amount and price are assumed to be valid"),
						available,
					),
					amount,
				)
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
	) -> Result<PositionInfo, PositionError<MintError>> {
		let sqrt_price = Self::validate_tick(tick)?;
		let orders = &mut self.orders[!SD::INPUT_SIDE];

		if amount.is_zero() {
			return orders
				.get(&sqrt_price)
				.and_then(|orders| orders.get(lp))
				.map(PositionInfo::from)
				.ok_or(PositionError::NonExistent)
		}

		// The maximum is checked before anything is mutated, so that a failed mint is a no-op.
		let available = orders.get(&sqrt_price).map_or(Amount::zero(), liquidity_of);
		if available.saturating_add(amount) > MAX_LIQUIDITY_PER_PRICE {
			return Err(PositionError::Other(MintError::MaximumLiquidity))
		}

		let position = orders.entry(sqrt_price).or_default().entry(lp.clone()).or_default();
		position.amount = position.amount.saturating_add(amount);
		position.original_amount = position.amount;

		Ok(PositionInfo::from(&*position))
	}

	fn validate_tick<T>(tick: Tick) -> Result<SqrtPrice, PositionError<T>> {
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
	) -> Result<(Amount, PositionInfo), PositionError<BurnError>> {
		let sqrt_price = Self::validate_tick(tick)?;

		let all_orders = &mut self.orders[!SD::INPUT_SIDE];
		let orders = all_orders.get_mut(&sqrt_price).ok_or(PositionError::NonExistent)?;
		let position = orders.get_mut(lp).ok_or(PositionError::NonExistent)?;

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
	) -> Result<PositionInfo, PositionError<Infallible>> {
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
		let start =
			Self::validate_tick::<Infallible>(range.start).map_err(|_| DepthError::InvalidTick)?;
		let end =
			Self::validate_tick::<Infallible>(range.end).map_err(|_| DepthError::InvalidTick)?;
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

		// Cannot underflow: `remaining_sold` never exceeds `remaining_available`, which bounds an
		// order's share of it by the liquidity that order provides.
		debug_assert!(sold <= position.amount);
		remaining_available = remaining_available.saturating_sub(position.amount);
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

		// An order with nothing left to sell is dropped; its proceeds are owed to the lp, not held
		// by the pool.
		!position.amount.is_zero()
	});
}
