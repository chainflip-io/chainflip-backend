use scale_info::TypeInfo;
use sp_core::{Decode, Encode};

#[derive(Clone, Debug, TypeInfo, Encode, Decode)]
pub struct PoolState<LiquidityProvider> {
	limit_orders: super::limit_orders::v1::PoolState<LiquidityProvider>,
	range_orders: super::range_orders::v1::PoolState<LiquidityProvider>,
}

impl<LiquidityProvider: Ord> From<PoolState<LiquidityProvider>>
	for super::PoolState<LiquidityProvider>
{
	fn from(value: PoolState<LiquidityProvider>) -> Self {
		Self { limit_orders: value.limit_orders.into(), range_orders: value.range_orders.into() }
	}
}
