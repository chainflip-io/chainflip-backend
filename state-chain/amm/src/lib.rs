#![cfg_attr(not(feature = "std"), no_std)]

pub mod test_utilities;
mod tests;

use core::convert::Infallible;

use codec::{Decode, Encode};
use common::{
	price_to_sqrt_price, sqrt_price_to_price, Amount, OneToZero, Order, Price, SetFeesError, Side,
	SideMap, SqrtPriceQ64F96, Tick, ZeroToOne,
};
use limit_orders::{Collected, PositionInfo};
use range_orders::Liquidity;
use scale_info::TypeInfo;
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

pub mod common;
pub mod limit_orders;
pub mod range_orders;
pub mod v1;

#[derive(Clone, Debug, TypeInfo, Encode, Decode)]
pub struct PoolState<LiquidityProvider> {
	limit_orders: limit_orders::PoolState<LiquidityProvider>,
	range_orders: range_orders::PoolState<LiquidityProvider>,
}

pub enum NewError {
	LimitOrders(limit_orders::NewError),
	RangeOrders(range_orders::NewError),
}

impl<LiquidityProvider: Clone + Ord> PoolState<LiquidityProvider> {
	pub fn new(
		fee_hundredth_pips: u32,
		initial_range_order_price: Price,
	) -> Result<Self, NewError> {
		Ok(Self {
			limit_orders: limit_orders::PoolState::new(fee_hundredth_pips)
				.map_err(NewError::LimitOrders)?,
			range_orders: range_orders::PoolState::new(
				fee_hundredth_pips,
				price_to_sqrt_price(initial_range_order_price),
			)
			.map_err(NewError::RangeOrders)?,
		})
	}

	/// Returns the current price for a given direction of swap. The price is measured in units of
	/// the specified Side argument
	pub fn current_price(&mut self, side: Side, order: Order) -> Option<Price> {
		match (side, order) {
			(Side::Zero, Order::Buy) => self.inner_current_price::<OneToZero>(),
			(Side::Zero, Order::Sell) => self.inner_current_price::<ZeroToOne>(),
			(Side::One, Order::Sell) => self.inner_current_price::<OneToZero>(),
			(Side::One, Order::Buy) => self.inner_current_price::<ZeroToOne>(),
		}
	}

	fn inner_current_price<
		SD: common::SwapDirection + limit_orders::SwapDirection + range_orders::SwapDirection,
	>(
		&mut self,
	) -> Option<Price> {
		match (
			self.limit_orders.current_sqrt_price::<SD>(),
			self.range_orders.current_sqrt_price::<SD>(),
		) {
			(Some(limit_order_sqrt_price), Some(range_order_sqrt_price)) =>
				if SD::sqrt_price_op_more_than(limit_order_sqrt_price, range_order_sqrt_price) {
					Some(range_order_sqrt_price)
				} else {
					Some(limit_order_sqrt_price)
				},
			(Some(limit_order_sqrt_price), None) => Some(limit_order_sqrt_price),
			(None, Some(range_order_sqrt_price)) => Some(range_order_sqrt_price),
			(None, None) => None,
		}
		.map(sqrt_price_to_price)
	}

	/// Performs a swap to sell or buy an amount of either side/asset.
	///
	/// This function never panics.
	pub fn swap(&mut self, side: Side, order: Order, amount: Amount) -> (Amount, Amount) {
		match (side, order) {
			(Side::Zero, Order::Sell) => self.inner_swap::<ZeroToOne>(amount, None),
			(Side::One, Order::Sell) => self.inner_swap::<OneToZero>(amount, None),
			(_, Order::Buy) => unimplemented!(), // We don't support exact buy swaps at the moment
		}
	}

	fn inner_swap<
		SD: common::SwapDirection + limit_orders::SwapDirection + range_orders::SwapDirection,
	>(
		&mut self,
		mut amount: Amount,
		sqrt_price_limit: Option<SqrtPriceQ64F96>,
	) -> (Amount, Amount) {
		let mut total_output_amount = Amount::zero();

		while !amount.is_zero() {
			let (output_amount, remaining_amount) = match (
				self.limit_orders.current_sqrt_price::<SD>().filter(|sqrt_price| {
					sqrt_price_limit.map_or(true, |sqrt_price_limit| {
						!SD::sqrt_price_op_more_than(*sqrt_price, sqrt_price_limit)
					})
				}),
				self.range_orders.current_sqrt_price::<SD>().filter(|sqrt_price| {
					sqrt_price_limit.map_or(true, |sqrt_price_limit| {
						SD::sqrt_price_op_more_than(sqrt_price_limit, *sqrt_price)
					})
				}),
			) {
				(Some(limit_order_sqrt_price), Some(range_order_sqrt_price)) => {
					if SD::sqrt_price_op_more_than(limit_order_sqrt_price, range_order_sqrt_price) {
						self.range_orders.swap::<SD>(amount, Some(limit_order_sqrt_price))
					} else {
						// Note it is important that in the equal price case we prefer to swap limit
						// orders as if we do a swap with range_orders where the sqrt_price_limit is
						// equal to the current sqrt_price then the swap will not change the current
						// price or use any of the input amount, therefore we would loop forever

						// Also we prefer limit orders as they don't immediately incur slippage
						self.limit_orders.swap::<SD>(amount, Some(range_order_sqrt_price))
					}
				},
				(Some(_), None) => self.limit_orders.swap::<SD>(amount, sqrt_price_limit),
				(None, Some(_)) => self.range_orders.swap::<SD>(amount, sqrt_price_limit),
				(None, None) => break,
			};

			amount = remaining_amount;
			total_output_amount = total_output_amount.saturating_add(output_amount);
		}

		(total_output_amount, amount)
	}

	pub fn collect_and_mint_limit_order(
		&mut self,
		lp: &LiquidityProvider,
		side: Side,
		order: Order,
		tick: Tick,
		amount: Amount,
	) -> Result<
		(limit_orders::Collected, limit_orders::PositionInfo),
		limit_orders::PositionError<limit_orders::MintError>,
	> {
		match (side, order) {
			(Side::Zero, Order::Sell) | (Side::One, Order::Buy) =>
				self.inner_collect_and_mint_limit_order::<OneToZero>(lp, tick, side, amount),
			(Side::Zero, Order::Buy) | (Side::One, Order::Sell) =>
				self.inner_collect_and_mint_limit_order::<ZeroToOne>(lp, tick, side, amount),
		}
	}

	fn inner_collect_and_mint_limit_order<SD: limit_orders::SwapDirection>(
		&mut self,
		lp: &LiquidityProvider,
		tick: Tick,
		amount_units: Side,
		amount: Amount,
	) -> Result<
		(limit_orders::Collected, limit_orders::PositionInfo),
		limit_orders::PositionError<limit_orders::MintError>,
	> {
		self.limit_orders.collect_and_mint::<SD>(lp, tick, {
			if amount_units == SD::INPUT_SIDE {
				SD::input_to_output_amount_floor(amount, tick)
					.ok_or(limit_orders::PositionError::InvalidTick)?
			} else {
				amount
			}
		})
	}

	pub fn collect_and_burn_limit_order(
		&mut self,
		lp: &LiquidityProvider,
		side: Side,
		order: Order,
		tick: Tick,
		amount: Amount,
	) -> Result<
		(Amount, limit_orders::Collected, limit_orders::PositionInfo),
		limit_orders::PositionError<limit_orders::BurnError>,
	> {
		match (side, order) {
			(Side::Zero, Order::Sell) | (Side::One, Order::Buy) =>
				self.inner_collect_and_burn_limit_order::<OneToZero>(lp, tick, side, amount),
			(Side::Zero, Order::Buy) | (Side::One, Order::Sell) =>
				self.inner_collect_and_burn_limit_order::<ZeroToOne>(lp, tick, side, amount),
		}
	}

	fn inner_collect_and_burn_limit_order<SD: limit_orders::SwapDirection>(
		&mut self,
		lp: &LiquidityProvider,
		tick: Tick,
		amount_units: Side,
		amount: Amount,
	) -> Result<
		(Amount, limit_orders::Collected, limit_orders::PositionInfo),
		limit_orders::PositionError<limit_orders::BurnError>,
	> {
		self.limit_orders.collect_and_burn::<SD>(lp, tick, {
			if amount_units == SD::INPUT_SIDE {
				SD::input_to_output_amount_floor(amount, tick)
					.ok_or(limit_orders::PositionError::InvalidTick)?
			} else {
				amount
			}
		})
	}

	pub fn collect_and_mint_range_order<T, E, TryDebit: FnOnce(SideMap<Amount>) -> Result<T, E>>(
		&mut self,
		lp: &LiquidityProvider,
		tick_range: core::ops::Range<Tick>,
		size: range_orders::Size,
		try_debit: TryDebit,
	) -> Result<
		(T, range_orders::Liquidity, range_orders::Collected, range_orders::PositionInfo),
		range_orders::PositionError<range_orders::MintError<E>>,
	> {
		self.range_orders
			.collect_and_mint(lp, tick_range.start, tick_range.end, size, try_debit)
	}

	pub fn collect_and_burn_range_order(
		&mut self,
		lp: &LiquidityProvider,
		tick_range: core::ops::Range<Tick>,
		size: range_orders::Size,
	) -> Result<
		(
			SideMap<Amount>,
			range_orders::Liquidity,
			range_orders::Collected,
			range_orders::PositionInfo,
		),
		range_orders::PositionError<range_orders::BurnError>,
	> {
		self.range_orders.collect_and_burn(lp, tick_range.start, tick_range.end, size)
	}

	pub fn range_order_liquidity_value(
		&self,
		tick_range: core::ops::Range<Tick>,
		liquidity: Liquidity,
	) -> Result<SideMap<Amount>, range_orders::LiquidityToAmountsError> {
		self.range_orders
			.liquidity_to_amounts::<true>(liquidity, tick_range.start, tick_range.end)
	}

	pub fn required_asset_ratio_for_range_order(
		&self,
		tick_range: core::ops::Range<Tick>,
	) -> Result<SideMap<Amount>, range_orders::RequiredAssetRatioError> {
		self.range_orders
			.required_asset_ratio::<false>(tick_range.start, tick_range.end)
	}

	pub fn range_order(
		&self,
		lp: &LiquidityProvider,
		tick_range: core::ops::Range<Tick>,
	) -> Result<
		(range_orders::Collected, range_orders::PositionInfo),
		range_orders::PositionError<Infallible>,
	> {
		self.range_orders.position(lp, tick_range.start, tick_range.end)
	}

	pub fn limit_order(
		&self,
		lp: &LiquidityProvider,
		side: Side,
		order: Order,
		tick: Tick,
	) -> Result<
		(limit_orders::Collected, limit_orders::PositionInfo),
		limit_orders::PositionError<Infallible>,
	> {
		match (side, order) {
			(Side::Zero, Order::Sell) | (Side::One, Order::Buy) =>
				self.limit_orders.position::<OneToZero>(lp, tick),
			(Side::Zero, Order::Buy) | (Side::One, Order::Sell) =>
				self.limit_orders.position::<ZeroToOne>(lp, tick),
		}
	}

	pub fn limit_order_fee(&self) -> u32 {
		self.limit_orders.fee_hundredth_pips
	}

	pub fn range_order_fee(&self) -> u32 {
		self.range_orders.fee_hundredth_pips
	}

	pub fn limit_order_liquidity(&self, side: Side, order: Order) -> Vec<(Tick, Amount)> {
		match (side, order) {
			(Side::Zero, Order::Sell) | (Side::One, Order::Buy) =>
				self.limit_orders.liquidity::<OneToZero>(),
			(Side::Zero, Order::Buy) | (Side::One, Order::Sell) =>
				self.limit_orders.liquidity::<ZeroToOne>(),
		}
	}

	pub fn range_order_liquidity(&self) -> Vec<(Tick, Liquidity)> {
		self.range_orders.liquidity()
	}

	pub fn limit_order_depth(
		&mut self,
		range: core::ops::Range<Tick>,
	) -> Result<SideMap<(Option<Price>, Amount)>, limit_orders::DepthError> {
		Ok(SideMap {
			zero: (
				self.limit_orders.current_sqrt_price::<OneToZero>(),
				self.limit_orders.depth::<OneToZero>(range.clone())?,
			),
			one: (
				self.limit_orders.current_sqrt_price::<ZeroToOne>(),
				self.limit_orders.depth::<ZeroToOne>(range)?,
			),
		})
	}

	pub fn range_order_depth(
		&self,
		range: core::ops::Range<Tick>,
	) -> Result<SideMap<(Option<Price>, Amount)>, range_orders::DepthError> {
		self.range_orders.depth(range.start, range.end).map(|assets| SideMap {
			zero: (
				self.range_orders.current_sqrt_price::<OneToZero>().map(sqrt_price_to_price),
				assets[Side::Zero],
			),
			one: (
				self.range_orders.current_sqrt_price::<ZeroToOne>().map(sqrt_price_to_price),
				assets[Side::One],
			),
		})
	}

	#[allow(clippy::type_complexity)]
	pub fn set_fees(
		&mut self,
		fee_hundredth_pips: u32,
	) -> Result<SideMap<BTreeMap<(Tick, LiquidityProvider), (Collected, PositionInfo)>>, SetFeesError>
	{
		self.range_orders.set_fees(fee_hundredth_pips)?;
		self.limit_orders.set_fees(fee_hundredth_pips)
	}

	// Returns if the pool fee is valid.
	pub fn validate_fees(fee_hundredth_pips: u32) -> bool {
		limit_orders::PoolState::<LiquidityProvider>::validate_fees(fee_hundredth_pips) &&
			range_orders::PoolState::<LiquidityProvider>::validate_fees(fee_hundredth_pips)
	}
}
