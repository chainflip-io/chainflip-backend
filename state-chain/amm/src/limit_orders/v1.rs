use sp_std::collections::btree_map::BTreeMap;

use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

use crate::{
	common::{Amount, SideMap, SqrtPriceQ64F96},
	limit_orders::FloatBetweenZeroAndOne,
};

#[derive(Clone, Debug, TypeInfo, Encode, Decode, MaxEncodedLen)]
struct Position {
	pool_instance: u128,
	amount: Amount,
	last_percent_remaining: FloatBetweenZeroAndOne,
}

#[derive(Clone, Debug, TypeInfo, Encode, Decode)]
pub struct PoolState<LiquidityProvider> {
	fee_hundredth_pips: u32,
	next_pool_instance: u128,
	fixed_pools: SideMap<BTreeMap<SqrtPriceQ64F96, super::FixedPool>>,
	positions: SideMap<BTreeMap<(SqrtPriceQ64F96, LiquidityProvider), Position>>,
}

impl<LiquidityProvider: Ord> From<PoolState<LiquidityProvider>>
	for super::PoolState<LiquidityProvider>
{
	fn from(v1_state: PoolState<LiquidityProvider>) -> Self {
		Self {
			fee_hundredth_pips: v1_state.fee_hundredth_pips,
			next_pool_instance: v1_state.next_pool_instance,
			fixed_pools: v1_state.fixed_pools,
			positions: v1_state.positions.map(|_side, positions| {
				positions
					.into_iter()
					.map(|((sqrt_price, lp), position)| {
						(
							(sqrt_price, lp),
							super::Position {
								pool_instance: position.pool_instance,
								amount: position.amount,
								last_percent_remaining: position.last_percent_remaining,
								accumulative_fees: Default::default(),
								original_amount: Default::default(),
							},
						)
					})
					.collect()
			}),
		}
	}
}
