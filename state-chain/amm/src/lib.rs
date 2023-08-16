#![cfg_attr(not(feature = "std"), no_std)]

pub mod test_utilities;
mod tests;

use codec::{Decode, Encode};
use common::{sqrt_price_to_price, Amount, Price, SqrtPriceQ64F96};
use scale_info::TypeInfo;

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

pub mod common;
pub mod limit_orders;
pub mod range_orders;

#[derive(Clone, Debug, TypeInfo, Encode, Decode)]
#[cfg_attr(feature = "std", derive(Deserialize, Serialize))]
#[cfg_attr(
	feature = "std",
	serde(bound = "LiquidityProvider: Clone + Ord + Serialize + serde::de::DeserializeOwned")
)]
pub struct PoolState<LiquidityProvider> {
	pub limit_orders: limit_orders::PoolState<LiquidityProvider>,
	pub range_orders: range_orders::PoolState<LiquidityProvider>,
}

impl<LiquidityProvider: Clone + Ord> PoolState<LiquidityProvider> {
	pub fn current_price<
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

	pub fn swap<
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
}
