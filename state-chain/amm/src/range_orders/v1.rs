use sp_std::collections::btree_map::BTreeMap;

use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

use super::{FeeGrowthQ128F128, Liquidity};
use crate::common::{SideMap, SqrtPriceQ64F96, Tick};

#[derive(Clone, Debug, TypeInfo, Encode, Decode, MaxEncodedLen)]
struct Position {
	liquidity: Liquidity,
	last_fee_growth_inside: SideMap<FeeGrowthQ128F128>,
}

#[derive(Clone, Debug, TypeInfo, Encode, Decode)]
pub struct PoolState<LiquidityProvider> {
	fee_hundredth_pips: u32,
	current_sqrt_price: SqrtPriceQ64F96,
	current_tick: Tick,
	current_liquidity: Liquidity,
	global_fee_growth: SideMap<FeeGrowthQ128F128>,
	liquidity_map: BTreeMap<Tick, super::TickDelta>,
	positions: BTreeMap<(LiquidityProvider, Tick, Tick), Position>,
}

impl<LiquidityProvider: Ord> From<PoolState<LiquidityProvider>>
	for super::PoolState<LiquidityProvider>
{
	fn from(v1_state: PoolState<LiquidityProvider>) -> Self {
		Self {
			fee_hundredth_pips: v1_state.fee_hundredth_pips,
			current_sqrt_price: v1_state.current_sqrt_price,
			current_tick: v1_state.current_tick,
			current_liquidity: v1_state.current_liquidity,
			global_fee_growth: v1_state.global_fee_growth,
			liquidity_map: v1_state.liquidity_map,
			positions: v1_state
				.positions
				.into_iter()
				.map(|((lp, lower_tick, upper_tick), position)| {
					(
						(lp, lower_tick, upper_tick),
						super::Position {
							liquidity: position.liquidity,
							last_fee_growth_inside: position.last_fee_growth_inside,
							accumulative_fees: Default::default(),
							original_sqrt_price: Default::default(),
						},
					)
				})
				.collect(),
			total_fees_earned: Default::default(),
			total_swap_inputs: Default::default(),
			total_swap_outputs: Default::default(),
		}
	}
}
