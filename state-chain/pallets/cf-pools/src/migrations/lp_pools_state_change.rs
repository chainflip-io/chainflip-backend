use crate::*;

use crate::Config;
use cf_amm::{
	limit_orders::Position as LimitOrdersPosition, range_orders::Position as RangeOrdersPosition,
};
use frame_support::traits::OnRuntimeUpgrade;
use sp_std::{collections::btree_map::BTreeMap, marker::PhantomData};

use crate::common::Pairs;
use cf_amm::common::price_at_tick;

#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::DispatchError;

#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

use cf_amm::{
	limit_orders::PoolState as LimitOrdersPoolState,
	range_orders::PoolState as RangeOrdersPoolState,
};

pub struct Migration<T: Config>(PhantomData<T>);

pub(crate) mod old {
	use super::*;

	use cf_amm::old::PoolState as OldPoolState;

	#[derive(Clone, Debug, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(T))]
	pub struct Pool<T: Config> {
		pub range_orders_cache: BTreeMap<T::AccountId, BTreeMap<OrderId, Range<Tick>>>,
		pub limit_orders_cache: PoolPairsMap<BTreeMap<T::AccountId, BTreeMap<OrderId, Tick>>>,
		pub pool_state: OldPoolState<(T::AccountId, OrderId)>,
	}

	#[frame_support::storage_alias]
	pub type Pools<T: Config> =
		StorageMap<Pallet<T>, Twox64Concat, AssetPair, Pool<T>, OptionQuery>;
}

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		log::info!("Migrating LP Pools state");
		old::Pools::<T>::drain().for_each(|(asset_pair, pool)| {
			let mut transformed_range_order_positions: BTreeMap<
				(T::AccountId, OrderId, Tick, Tick),
				RangeOrdersPosition,
			> = BTreeMap::new();
			let mut transformed_limit_orders: BTreeMap<
				Pairs,
				BTreeMap<(T::AccountId, OrderId, SqrtPriceQ64F96), LimitOrdersPosition>,
			> = BTreeMap::new();
			pool.range_orders_cache.iter().for_each(|(lp, range_orders)| {
				range_orders.iter().for_each(|(id, tick_range)| {
					let current_positions = pool
						.pool_state
						.range_orders
						.positions
						.get(&((lp.clone(), id.clone()), tick_range.start, tick_range.end))
						.unwrap();
					transformed_range_order_positions.insert(
						(lp.clone(), id.clone(), tick_range.start, tick_range.end),
						current_positions.clone(),
					);
				});
			});
			pool.limit_orders_cache.as_ref().into_iter().for_each(|(assets, limit_orders)| {
				limit_orders.iter().for_each(|(lp, limit_orders)| {
					let mut orders: BTreeMap<_, _> = BTreeMap::new();
					limit_orders.iter().for_each(|(id, tick)| {
						let price = price_at_tick(tick.clone()).unwrap();
						let current_positions = pool.pool_state.limit_orders.positions[assets]
							.get(&(price, (lp.clone(), id.clone())))
							.unwrap();
						orders.insert((lp.clone(), id.clone(), price), current_positions.clone());
					});
					transformed_limit_orders.insert(assets, orders);
				})
			});
			Pools::<T>::insert(
				asset_pair,
				Pool {
					pool_state: PoolState {
						range_orders: RangeOrdersPoolState::migrate(
							pool.pool_state.range_orders,
							transformed_range_order_positions,
						),
						limit_orders: if transformed_limit_orders.get(&Pairs::Base).is_some() &&
							transformed_limit_orders.get(&Pairs::Quote).is_some()
						{
							LimitOrdersPoolState::migrate(
								pool.pool_state.limit_orders,
								PoolPairsMap {
									base: transformed_limit_orders
										.get(&Pairs::Base)
										.unwrap()
										.clone(),
									quote: transformed_limit_orders
										.get(&Pairs::Quote)
										.unwrap()
										.clone(),
								},
							)
						} else {
							LimitOrdersPoolState::migrate(
								pool.pool_state.limit_orders,
								PoolPairsMap { base: BTreeMap::new(), quote: BTreeMap::new() },
							)
						},
					},
				},
			);
		});
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(Default::default())
	}

	#[cfg(feature = "try-runtime")]
	#[allow(clippy::bool_assert_comparison)]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		Ok(())
	}
}
