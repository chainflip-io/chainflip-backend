use crate::*;

use crate::Config;
use cf_amm::{
	limit_orders::Position as LimitOrdersPosition, range_orders::Position as RangeOrdersPosition,
};
use frame_support::traits::OnRuntimeUpgrade;
use sp_std::{collections::btree_map::BTreeMap, marker::PhantomData};

use crate::common::Pairs;

#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::DispatchError;

#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

use cf_amm::{
	limit_orders::PoolState as LimitOrdersPoolState,
	range_orders::PoolState as RangeOrdersPoolState,
};

use cf_amm::old::PoolState as OldPoolState;

pub struct Migration<T: Config>(PhantomData<T>);

pub(crate) mod old {
	use super::*;

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
		old::Pools::<T>::drain().for_each(|(asset_pair, pool)| {
			let mut transformed_range_order_positions: BTreeMap<
				(T::AccountId, OrderId, Tick, Tick),
				RangeOrdersPosition,
			> = BTreeMap::new();

			#[allow(clippy::type_complexity)]
			let mut transformed_limit_orders: BTreeMap<
				Pairs,
				BTreeMap<(T::AccountId, OrderId, SqrtPriceQ64F96), LimitOrdersPosition>,
			> = BTreeMap::new();

			pool.pool_state.limit_orders.positions.base.iter().for_each(|(key, value)| {
				let price = key.0;
				let lp = key.1 .0.clone();
				let id = key.1 .1;
				transformed_limit_orders
					.entry(Pairs::Base)
					.or_insert_with(BTreeMap::new)
					.insert((lp.clone(), id, price), value.clone());
			});

			pool.pool_state.limit_orders.positions.quote.iter().for_each(|(key, value)| {
				let price = key.0;
				let lp = key.1 .0.clone();
				let id = key.1 .1;
				transformed_limit_orders
					.entry(Pairs::Quote)
					.or_insert_with(BTreeMap::new)
					.insert((lp.clone(), id, price), value.clone());
			});

			pool.pool_state.range_orders.positions.iter().for_each(|(key, value)| {
				let lp = key.0 .0.clone();
				let id = key.0 .1;
				let start = key.1;
				let end = key.2;
				transformed_range_order_positions.insert((lp, id, start, end), value.clone());
			});

			let no_orders: BTreeMap<(T::AccountId, OrderId, SqrtPriceQ64F96), LimitOrdersPosition> =
				BTreeMap::new();
			Pools::<T>::insert(
				asset_pair,
				Pool {
					pool_state: PoolState {
						range_orders: RangeOrdersPoolState::migrate(
							pool.pool_state.range_orders,
							transformed_range_order_positions,
						),
						limit_orders: LimitOrdersPoolState::migrate(
							pool.pool_state.limit_orders,
							PoolPairsMap {
								base: transformed_limit_orders
									.get(&Pairs::Base)
									.unwrap_or(&no_orders)
									.clone(),
								quote: transformed_limit_orders
									.get(&Pairs::Quote)
									.unwrap_or(&no_orders)
									.clone(),
							},
						),
					},
				},
			);
		});
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let mut old_pool_state: BTreeMap<AssetPair, old::Pool<T>> = BTreeMap::new();
		old::Pools::<T>::iter().for_each(|(asset_pair, pool)| {
			old_pool_state.insert(asset_pair, pool);
		});
		Ok(old_pool_state.encode())
	}

	#[cfg(feature = "try-runtime")]
	#[allow(clippy::bool_assert_comparison)]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let old_pool_state: BTreeMap<AssetPair, old::Pool<T>> =
			<BTreeMap<AssetPair, old::Pool<T>>>::decode(&mut &state[..])
				.map_err(|_| "Failed to decode pre-upgrade state.")?;

		Pools::<T>::iter().for_each(|(asset_pair, pool)| {
			let old_pool = old_pool_state.get(&asset_pair).expect("pool should exist in old state");
			let new_range_orders_state = pool.pool_state.range_orders.clone();
			let new_limit_orders_state = pool.pool_state.limit_orders.clone();
			let old_range_orders_state = &old_pool.clone().pool_state.range_orders;
			let old_limit_orders_state = &old_pool.pool_state.limit_orders;

			assert!(
				pool.pool_state.range_orders.is_state_migrated(old_range_orders_state),
				"Range orders state should be migrated"
			);

			assert!(
				pool.pool_state.limit_orders.is_state_migrated(old_limit_orders_state),
				"Limit orders state should be migrated"
			);

			assert_eq!(
				new_range_orders_state.positions.len(),
				old_range_orders_state.positions.len(),
				"Range orders positions count mismatch"
			);

			assert_eq!(
				new_limit_orders_state.positions.base.len(),
				old_limit_orders_state.positions.base.len(),
				"Limit orders base positions count mismatch"
			);

			assert_eq!(
				new_limit_orders_state.positions.quote.len(),
				old_limit_orders_state.positions.quote.len(),
				"Limit orders quote positions count mismatch"
			);

			old_range_orders_state.positions.iter().for_each(|(key, value)| {
				let new_key: (T::AccountId, OrderId, Tick, Tick) =
					(key.0 .0.clone(), key.0 .1, key.1, key.2);
				assert_eq!(
					new_range_orders_state
						.positions
						.get(&new_key)
						.expect("positions to be available")
						.encode(),
					value.encode(),
					"Range orders positions mismatch"
				);
			});

			old_limit_orders_state.positions.base.iter().for_each(|(key, value)| {
				let new_key: (T::AccountId, OrderId, SqrtPriceQ64F96) =
					(key.1 .0.clone(), key.1 .1, key.0);
				assert_eq!(
					new_limit_orders_state
						.positions
						.base
						.get(&new_key)
						.expect("positions to be available")
						.encode(),
					value.encode(),
					"Limit orders base positions mismatch"
				);
			});

			old_limit_orders_state.positions.quote.iter().for_each(|(key, value)| {
				let new_key: (T::AccountId, OrderId, SqrtPriceQ64F96) =
					(key.1 .0.clone(), key.1 .1, key.0);
				assert_eq!(
					new_limit_orders_state
						.positions
						.quote
						.get(&new_key)
						.expect("positions to be available")
						.encode(),
					value.encode(),
					"Limit orders quote positions mismatch"
				);
			});
		});

		Ok(())
	}
}
