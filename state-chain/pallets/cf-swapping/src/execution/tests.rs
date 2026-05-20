
use super::*;
use crate::{
	execution::{GroupExecutionPhase, SwapLeg},
	mock::Test,
};
use proptest::prelude::*;
use strum::IntoEnumIterator;

fn any_asset() -> impl Strategy<Value = Asset> {
	proptest::sample::select(Asset::iter().collect::<Vec<_>>())
}

/// Sanity check to ensure that we don't try to execute on pools that don't exist. This will
/// need to be updated as we add pools.
fn pool_exists_for_swap_leg(leg: SwapLeg) -> bool {
	match (leg.from, leg.to) {
		(Asset::Btc, Asset::Wbtc) | (Asset::Wbtc, Asset::Btc) if ENABLE_WBTC_BTC_ROUTE => true,
		(Asset::Usdc, _) | (_, Asset::Usdc) => true,
		_ => false,
	}
}

proptest! {
	#[test]
	/// Test that for any given input and output asset, the routing and grouping logic will complete
	/// the swap within the 4 execution phases using only available pools. This does not test the
	/// actual swap execution logic, just the routing and grouping logic.
	/// It also checks that the route returned by get_swap_route matches the legs that were actually
	/// executed.
	fn all_swap_routes_complete(
		starting_asset in any_asset(),
		final_asset in any_asset(),
	) {
		let mut input = starting_asset;
		let mut executed_legs = Vec::new();
		for phase in GroupExecutionPhase::iter() {
			if let Some(leg) = Pallet::<Test>::get_next_swap_leg(input, final_asset) {
				prop_assert!(
					pool_exists_for_swap_leg(leg.clone()),
					"Routing logic produced a swap leg for an invalid pool: {:?} -> {:?}",
					leg.from,
					leg.to,
				);
				if Pallet::<Test>::should_group_leg_execute(leg.from, leg.to, phase) {
					input = leg.to;
					executed_legs.push(leg);
				}
			}
		}
		prop_assert_eq!(
			input, final_asset,
			"Swap from {:?} to {:?} did not reach the final asset",
			starting_asset,
			final_asset,
		);

		// get_swap_route should agree with the legs produced by phase execution.
		let route = Pallet::<Test>::get_swap_route(starting_asset, final_asset);
		prop_assert_eq!(route, executed_legs);
	}
}
