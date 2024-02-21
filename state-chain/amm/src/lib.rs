#![cfg_attr(not(feature = "std"), no_std)]

pub mod test_utilities;
mod tests;

use core::convert::Infallible;

use codec::{Decode, Encode};
use common::{
	is_sqrt_price_valid, price_to_sqrt_price, sqrt_price_to_price, tick_at_sqrt_price, Amount,
	BaseToQuote, Pairs, PoolPairsMap, Price, QuoteToBase, SetFeesError, Side, SqrtPriceQ64F96,
	SwapDirection, Tick,
};
use limit_orders::{Collected, PositionInfo};
use range_orders::Liquidity;
use scale_info::TypeInfo;
use sp_std::vec::Vec;

use crate::common::{mul_div_floor, nth_root_of_integer_as_fixed_point};

pub mod common;
pub mod limit_orders;
pub mod range_orders;

#[derive(Clone, Debug, TypeInfo, Encode, Decode, serde::Serialize, serde::Deserialize)]
pub struct PoolState<LiquidityProvider: Ord> {
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
	/// the specified Pairs argument
	pub fn current_price(&mut self, order: Side) -> Option<(Price, SqrtPriceQ64F96, Tick)> {
		self.current_sqrt_price(order).map(|sqrt_price| {
			(sqrt_price_to_price(sqrt_price), sqrt_price, tick_at_sqrt_price(sqrt_price))
		})
	}

	/// Returns the current sqrt price for a given direction of swap. The price is measured in units
	/// of the specified Pairs argument
	pub fn current_sqrt_price(&mut self, order: Side) -> Option<SqrtPriceQ64F96> {
		match order.to_sold_pair() {
			Pairs::Base => self.inner_current_sqrt_price::<BaseToQuote>(),
			Pairs::Quote => self.inner_current_sqrt_price::<QuoteToBase>(),
		}
	}

	fn inner_worst_price(order: Side) -> SqrtPriceQ64F96 {
		match order.to_sold_pair() {
			Pairs::Quote => QuoteToBase::WORST_SQRT_PRICE,
			Pairs::Base => BaseToQuote::WORST_SQRT_PRICE,
		}
	}

	pub fn logarithm_sqrt_price_sequence(
		&mut self,
		order: Side,
		count: u32,
	) -> Vec<SqrtPriceQ64F96> {
		let worst_sqrt_price = Self::inner_worst_price(order);
		if let Some(current_sqrt_price) = self
			.current_sqrt_price(order)
			.filter(|current_sqrt_price| *current_sqrt_price != worst_sqrt_price)
		{
			if worst_sqrt_price < current_sqrt_price {
				Some(count)
					.filter(move |count| *count > 1)
					.into_iter()
					.flat_map(|count| {
						let root = nth_root_of_integer_as_fixed_point(
							current_sqrt_price / worst_sqrt_price,
							count,
						);

						(0..(count - 1)).scan(current_sqrt_price, move |sqrt_price, _| {
							*sqrt_price =
								mul_div_floor(*sqrt_price, SqrtPriceQ64F96::one() << 128, root);
							Some(*sqrt_price)
						})
					})
					.chain(sp_std::iter::once(worst_sqrt_price))
					.collect()
			} else {
				Some(count)
					.filter(move |count| *count > 1)
					.into_iter()
					.flat_map(|count| {
						let root = nth_root_of_integer_as_fixed_point(
							worst_sqrt_price / current_sqrt_price,
							count,
						);

						(0..(count - 1)).scan(current_sqrt_price, move |sqrt_price, _| {
							*sqrt_price =
								mul_div_floor(*sqrt_price, root, SqrtPriceQ64F96::one() << 128);
							Some(*sqrt_price)
						})
					})
					.chain(sp_std::iter::once(worst_sqrt_price))
					.collect()
			}
		} else {
			Default::default()
		}
	}

	pub fn relative_sqrt_price(
		&self,
		order: Side,
		sqrt_price: SqrtPriceQ64F96,
		delta: Tick,
	) -> Option<SqrtPriceQ64F96> {
		if is_sqrt_price_valid(sqrt_price) {
			Some(match order {
				Side::Buy => QuoteToBase::increase_sqrt_price(sqrt_price, delta),
				Side::Sell => BaseToQuote::increase_sqrt_price(sqrt_price, delta),
			})
		} else {
			None
		}
	}

	fn inner_current_sqrt_price<
		SD: common::SwapDirection + limit_orders::SwapDirection + range_orders::SwapDirection,
	>(
		&mut self,
	) -> Option<SqrtPriceQ64F96> {
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
	}

	/// Performs a swap to sell or buy an amount of either side/asset.
	///
	/// This function never panics.
	pub fn swap(
		&mut self,
		order: Side,
		sold_amount: Amount,
		sqrt_price_limit: Option<SqrtPriceQ64F96>,
	) -> (Amount, Amount) {
		match order.to_sold_pair() {
			Pairs::Base => self.inner_swap::<BaseToQuote>(sold_amount, sqrt_price_limit),
			Pairs::Quote => self.inner_swap::<QuoteToBase>(sold_amount, sqrt_price_limit),
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
		order: Side,
		tick: Tick,
		sold_amount: Amount,
	) -> Result<
		(limit_orders::Collected, limit_orders::PositionInfo),
		limit_orders::PositionError<limit_orders::MintError>,
	> {
		match order.to_sold_pair() {
			Pairs::Base => self.limit_orders.collect_and_mint::<QuoteToBase>(lp, tick, sold_amount),
			Pairs::Quote =>
				self.limit_orders.collect_and_mint::<BaseToQuote>(lp, tick, sold_amount),
		}
	}

	pub fn collect_and_burn_limit_order(
		&mut self,
		lp: &LiquidityProvider,
		order: Side,
		tick: Tick,
		sold_amount: Amount,
	) -> Result<
		(Amount, limit_orders::Collected, limit_orders::PositionInfo),
		limit_orders::PositionError<limit_orders::BurnError>,
	> {
		match order.to_sold_pair() {
			Pairs::Base => self.limit_orders.collect_and_burn::<QuoteToBase>(lp, tick, sold_amount),
			Pairs::Quote =>
				self.limit_orders.collect_and_burn::<BaseToQuote>(lp, tick, sold_amount),
		}
	}

	pub fn collect_and_mint_range_order<
		T,
		E,
		TryDebit: FnOnce(PoolPairsMap<Amount>) -> Result<T, E>,
	>(
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
			PoolPairsMap<Amount>,
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
	) -> Result<PoolPairsMap<Amount>, range_orders::LiquidityToAmountsError> {
		self.range_orders
			.liquidity_to_amounts::<true>(liquidity, tick_range.start, tick_range.end)
	}

	pub fn required_asset_ratio_for_range_order(
		&self,
		tick_range: core::ops::Range<Tick>,
	) -> Result<PoolPairsMap<Amount>, range_orders::RequiredAssetRatioError> {
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

	pub fn range_orders(
		&self,
	) -> impl '_
	       + Iterator<
		Item = (
			LiquidityProvider,
			core::ops::Range<Tick>,
			range_orders::Collected,
			range_orders::PositionInfo,
		),
	> {
		self.range_orders.positions().map(
			|(lp, lower_tick, upper_tick, collected, position_info)| {
				(lp, lower_tick..upper_tick, collected, position_info)
			},
		)
	}

	pub fn limit_order(
		&self,
		lp: &LiquidityProvider,
		order: Side,
		tick: Tick,
	) -> Result<
		(limit_orders::Collected, limit_orders::PositionInfo),
		limit_orders::PositionError<Infallible>,
	> {
		match order {
			Side::Sell => self.limit_orders.position::<QuoteToBase>(lp, tick),
			Side::Buy => self.limit_orders.position::<BaseToQuote>(lp, tick),
		}
	}

	pub fn limit_orders(
		&self,
		order: Side,
	) -> sp_std::boxed::Box<
		dyn '_
			+ Iterator<
				Item = (
					LiquidityProvider,
					Tick,
					limit_orders::Collected,
					limit_orders::PositionInfo,
				),
			>,
	> {
		match order {
			Side::Sell => sp_std::boxed::Box::new(self.limit_orders.positions::<QuoteToBase>()),
			Side::Buy => sp_std::boxed::Box::new(self.limit_orders.positions::<BaseToQuote>()),
		}
	}

	pub fn limit_order_fee(&self) -> u32 {
		self.limit_orders.fee_hundredth_pips
	}

	pub fn range_order_fee(&self) -> u32 {
		self.range_orders.fee_hundredth_pips
	}

	pub fn range_order_total_fees_earned(&self) -> PoolPairsMap<Amount> {
		self.range_orders.total_fees_earned
	}

	pub fn limit_order_total_fees_earned(&self) -> PoolPairsMap<Amount> {
		self.limit_orders.total_fees_earned
	}

	pub fn range_order_swap_inputs(&self) -> PoolPairsMap<Amount> {
		self.range_orders.total_swap_inputs
	}

	pub fn limit_order_swap_inputs(&self) -> PoolPairsMap<Amount> {
		self.limit_orders.total_swap_inputs
	}

	pub fn limit_order_liquidity(&self, order: Side) -> Vec<(Tick, Amount)> {
		match order {
			Side::Sell => self.limit_orders.liquidity::<QuoteToBase>(),
			Side::Buy => self.limit_orders.liquidity::<BaseToQuote>(),
		}
	}

	pub fn range_order_liquidity(&self) -> Vec<(Tick, Liquidity)> {
		self.range_orders.liquidity()
	}

	pub fn limit_order_depth(
		&mut self,
		range: core::ops::Range<Tick>,
	) -> Result<PoolPairsMap<(Option<Price>, Amount)>, limit_orders::DepthError> {
		Ok(PoolPairsMap {
			base: (
				self.limit_orders.current_sqrt_price::<QuoteToBase>(),
				self.limit_orders.depth::<QuoteToBase>(range.clone())?,
			),
			quote: (
				self.limit_orders.current_sqrt_price::<BaseToQuote>(),
				self.limit_orders.depth::<BaseToQuote>(range)?,
			),
		})
	}

	pub fn range_order_depth(
		&self,
		range: core::ops::Range<Tick>,
	) -> Result<PoolPairsMap<(Option<Price>, Amount)>, range_orders::DepthError> {
		self.range_orders.depth(range.start, range.end).map(|assets| PoolPairsMap {
			base: (
				self.range_orders.current_sqrt_price::<QuoteToBase>().map(sqrt_price_to_price),
				assets[Pairs::Base],
			),
			quote: (
				self.range_orders.current_sqrt_price::<BaseToQuote>().map(sqrt_price_to_price),
				assets[Pairs::Quote],
			),
		})
	}

	#[allow(clippy::type_complexity)]
	pub fn set_fees(
		&mut self,
		fee_hundredth_pips: u32,
	) -> Result<PoolPairsMap<Vec<(LiquidityProvider, Tick, Collected, PositionInfo)>>, SetFeesError>
	{
		self.range_orders.set_fees(fee_hundredth_pips)?;
		self.limit_orders.set_fees(fee_hundredth_pips)
	}

	pub fn collect_all_range_orders(
		&mut self,
	) -> Vec<(
		LiquidityProvider,
		core::ops::Range<Tick>,
		range_orders::Collected,
		range_orders::PositionInfo,
	)> {
		self.range_orders
			.collect_all()
			.map(|((lp, lower_tick, upper_tick), (collected, position_info))| {
				(lp, lower_tick..upper_tick, collected, position_info)
			})
			.collect()
	}

	pub fn collect_all_limit_orders(
		&mut self,
	) -> PoolPairsMap<
		Vec<(LiquidityProvider, Tick, limit_orders::Collected, limit_orders::PositionInfo)>,
	> {
		self.limit_orders.collect_all()
	}

	// Returns if the pool fee is valid.
	pub fn validate_fees(fee_hundredth_pips: u32) -> bool {
		limit_orders::PoolState::<LiquidityProvider>::validate_fees(fee_hundredth_pips) &&
			range_orders::PoolState::<LiquidityProvider>::validate_fees(fee_hundredth_pips)
	}
}
