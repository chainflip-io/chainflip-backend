#[cfg(test)]
mod tests;

use codec::{Decode, Encode};
use common::{Amount, SqrtPriceQ64F96};
use scale_info::TypeInfo;

mod common;
mod limit_orders;
mod range_orders;

#[derive(Clone, Debug, TypeInfo, Encode, Decode)]
pub struct PoolState {
	pub limit_orders: limit_orders::PoolState,
	pub range_orders: range_orders::PoolState,
}
impl PoolState {
	pub fn swap<
		SD: common::SwapDirection + limit_orders::SwapDirection + range_orders::SwapDirection,
	>(
		&mut self,
		mut amount: Amount,
		sqrt_price_limit: Option<SqrtPriceQ64F96>,
	) -> (Amount, Amount) {
		let mut total_output_amount = Amount::zero();

		while amount != Amount::zero() {
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
