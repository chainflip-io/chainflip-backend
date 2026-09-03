// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

//! Reshapes each pool's limit orders now that fixed pools are gone, and pays out what those orders
//! had earned.
//!
//! Paying out is not optional. The old representation kept an order's earnings implicitly, as the
//! ratio between its price's `percent_remaining` and the order's own snapshot of it, and an order
//! bought in its entirety stayed in the pool until its lp collected. The new representation has
//! neither: proceeds are paid out by the swap that fills an order, and an order with nothing left
//! to sell does not exist. So everything outstanding is settled here, exactly as sweeping every
//! limit order immediately before the upgrade would have done.

use crate::{Config, Pallet, Pool, Pools};
use cf_amm::{
	common::{Pairs, PoolPairsMap, Side},
	limit_orders::migration_support::{Migrated, PoolStateV9, UncollectedProceeds},
	math::{Amount, Tick},
	range_orders,
};
use cf_primitives::{Asset, AssetAmount, OrderId, STABLE_ASSET};
use cf_runtime_utilities::log_or_panic;
use cf_traits::{BalanceApi, LpStatsApi};
use codec::{Decode, Encode};
use frame_support::{
	traits::{ConstU32, Get, UncheckedOnRuntimeUpgrade},
	weights::Weight,
	BoundedBTreeMap,
};
use sp_std::{collections::btree_map::BTreeMap, marker::PhantomData, ops::Range};

// Reading storage in its pre-migration shape is only needed by `pre_upgrade` and by the tests, so
// the alias and the pieces that declare it are gated to those.
#[cfg(any(test, feature = "try-runtime"))]
use cf_amm::common::AssetPair;
#[cfg(feature = "try-runtime")]
use cf_amm::limit_orders::migration_support::OrderBefore;
#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::DispatchError;
#[cfg(any(test, feature = "try-runtime"))]
use frame_support::{pallet_prelude::OptionQuery, Twox64Concat};
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

type LimitOrdersCache<T> =
	PoolPairsMap<BTreeMap<<T as frame_system::Config>::AccountId, BTreeMap<OrderId, Tick>>>;

mod old {
	use super::*;

	#[derive(Encode, Decode)]
	pub struct Pool<T: Config> {
		pub range_orders_cache: BTreeMap<T::AccountId, BTreeMap<OrderId, Range<Tick>>>,
		pub limit_orders_cache: LimitOrdersCache<T>,
		pub pool_state: PoolState<T>,
	}

	#[derive(Encode, Decode)]
	pub struct PoolState<T: Config> {
		pub limit_orders: PoolStateV9<(T::AccountId, OrderId)>,
		pub range_orders: range_orders::PoolState<(T::AccountId, OrderId)>,
	}

	/// The pools in their pre-migration shape
	#[cfg(any(test, feature = "try-runtime"))]
	#[frame_support::storage_alias]
	pub type Pools<T: Config> =
		StorageMap<Pallet<T>, Twox64Concat, AssetPair, Pool<T>, OptionQuery>;

	/// To be removed
	#[frame_support::storage_alias]
	pub type LimitOrderAutoSweepingThresholds<T: Config> =
		StorageValue<Pallet<T>, BoundedBTreeMap<Asset, AssetAmount, ConstU32<100>>>;
}

/// Every order a pool held before the conversion, by the pair it sells.
#[cfg(feature = "try-runtime")]
type OrdersBefore<T> =
	PoolPairsMap<Vec<OrderBefore<(<T as frame_system::Config>::AccountId, OrderId)>>>;

/// The liquidity a pool was offering at each price before the conversion, by the pair being sold.
#[cfg(feature = "try-runtime")]
type AvailableBefore = PoolPairsMap<Vec<(Tick, Amount)>>;

/// What each pool looked like before the conversion, so `post_upgrade` can hold the new state
/// against it order by order rather than only in aggregate.
#[cfg(feature = "try-runtime")]
#[derive(Encode, Decode)]
pub struct PoolBefore<T: Config> {
	pub available: AvailableBefore,
	pub orders: OrdersBefore<T>,
	/// The range order half encoded, which this migration must leave untouched.
	pub range_orders: Vec<u8>,
}

pub struct Migration<T>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		old::LimitOrderAutoSweepingThresholds::<T>::kill();

		let (mut pools, mut payouts, mut unsettled) = (0u64, 0u64, 0u64);
		let mut dust = PoolPairsMap::<Amount>::default();

		Pools::<T>::translate::<old::Pool<T>, _>(|asset_pair, old_pool| {
			pools = pools.saturating_add(1);

			let Migrated { pool_state: limit_orders, proceeds, dropped_dust, unconvertible } =
				old_pool.pool_state.limit_orders.migrate();
			let assets = asset_pair.assets();

			// Log uncollected dust
			for sold_pair in [Pairs::Base, Pairs::Quote] {
				if !dropped_dust[sold_pair].is_zero() {
					log::info!(
						"Dropping {} of unowned {:?} at {:?}.",
						dropped_dust[sold_pair],
						assets[sold_pair],
						asset_pair,
					);
				}
				dust[sold_pair] = dust[sold_pair].saturating_add(dropped_dust[sold_pair]);
			}

			// Report anything that would not convert, in full. Such an order is dropped rather
			// than carried over, so its lp is owed both what it still held and anything it had
			// earned, to be worked out and paid by hand.
			for order in unconvertible {
				unsettled = unsettled.saturating_add(1);
				log::error!(
					"Limit order in {asset_pair:?} was DROPPED because its recorded share of its \
					 price did not convert. Its lp must be repaid by hand, from: {order:?}"
				);
			}

			// Pay out filled orders
			for UncollectedProceeds {
				lp: (account_id, _),
				sold_pair,
				sold_amount,
				bought_amount,
			} in proceeds
			{
				// The order was selling `sold_pair`, so what it earned is the other one.
				let bought_asset = assets[!sold_pair];

				match AssetAmount::try_from(bought_amount) {
					Ok(0) => {},
					Ok(amount) => {
						payouts = payouts.saturating_add(1);
						T::LpBalance::credit_account(&account_id, bought_asset, amount);
						T::LpStats::on_limit_order_filled(
							&account_id,
							&bought_asset,
							if bought_asset == STABLE_ASSET {
								amount
							} else {
								sold_amount.try_into().unwrap_or_else(|_| {
									log_or_panic!(
										"Limit order sold amount of {} does not fit an AssetAmount",
										sold_amount
									);
									0
								})
							},
						);
					},
					Err(_) => log_or_panic!(
						"Uncollected limit order proceeds of {} do not fit an AssetAmount",
						bought_amount
					),
				}
			}

			let pool_state =
				cf_amm::PoolState { limit_orders, range_orders: old_pool.pool_state.range_orders };

			Some(Pool {
				range_orders_cache: old_pool.range_orders_cache,
				// Orders bought in their entirety no longer exist, so the pallet's index of open
				// orders is rebuilt from what survived rather than pruned. A stale entry here
				// would outlive the order it points at, and the lookups that trust it are not
				// fallible.
				limit_orders_cache: build_limit_orders_cache::<T>(&pool_state),
				pool_state,
			})
		});

		log::info!(
			"Removed fixed pools from {pools} pool(s), settling {payouts} order(s) and writing off \
			 {} base / {} quote of unowned liquidity.",
			dust[Pairs::Base],
			dust[Pairs::Quote],
		);
		if unsettled > 0 {
			log::error!(
				"{unsettled} limit order(s) could not be converted and were dropped. Each is \
				 reported above with everything needed to repay its lp by hand."
			);
		}

		// A read and write per pool, a read and write per lp paid out, and the killed thresholds.
		let touched = pools.saturating_add(payouts);
		T::DbWeight::get().reads_writes(touched, touched.saturating_add(1))
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(old::Pools::<T>::iter()
			.map(|(asset_pair, pool)| {
				(
					asset_pair,
					PoolBefore::<T> {
						available: pool.pool_state.limit_orders.available_by_price(),
						orders: pool.pool_state.limit_orders.orders(),
						range_orders: pool.pool_state.range_orders.encode(),
					},
				)
			})
			.collect::<Vec<_>>()
			.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let before = <Vec<(AssetPair, PoolBefore<T>)>>::decode(&mut &state[..])
			.map_err(|_| DispatchError::Other("Failed to decode the pre-upgrade state"))?;

		frame_support::ensure!(
			!old::LimitOrderAutoSweepingThresholds::<T>::exists(),
			"The auto sweeping thresholds should have been removed"
		);
		frame_support::ensure!(
			Pools::<T>::iter_keys().count() == before.len(),
			"Wrong number of pools after migration"
		);

		for (asset_pair, before) in before {
			let pool = Pools::<T>::get(asset_pair)
				.ok_or(DispatchError::Other("A pool was lost during the migration"))?;

			// The range order half is not this migration's business, so it must come through byte
			// for byte.
			frame_support::ensure!(
				pool.pool_state.range_orders.encode() == before.range_orders,
				"The range orders were altered by the migration"
			);
			frame_support::ensure!(
				pool.limit_orders_cache == build_limit_orders_cache::<T>(&pool.pool_state),
				"Limit order cache not updated"
			);

			for (side, sold_pair) in [(Side::Sell, Pairs::Base), (Side::Buy, Pairs::Quote)] {
				let orders_before = before.orders[sold_pair]
					.iter()
					.map(|order| {
						((order.tick, order.lp.clone()), (order.amount, order.original_amount))
					})
					.collect::<BTreeMap<_, _>>();

				// An order can shrink or disappear, but it cannot grow, change the size recorded
				// as of its last update, or turn up somewhere it never was.
				for (lp, tick, position_info) in pool.pool_state.limit_orders(side) {
					let (amount_before, original_before) = orders_before
						.get(&(tick, lp))
						.ok_or(DispatchError::Other("The migration invented an order"))?;

					frame_support::ensure!(
						!position_info.amount.is_zero(),
						"An order with no liquidity left survived the migration"
					);
					frame_support::ensure!(
						position_info.amount <= *amount_before,
						"An order came out of the migration holding more than it went in with"
					);
					frame_support::ensure!(
						position_info.original_amount == *original_before,
						"An order's size as of its last update was not preserved"
					);
				}

				// Per price rather than per pool: liquidity may be dropped where no order backed
				// it, but a price must never end up offering more than it was.
				let available_before =
					before.available[sold_pair].iter().copied().collect::<BTreeMap<Tick, Amount>>();

				for (tick, available_after) in pool.pool_state.limit_order_liquidity(side) {
					frame_support::ensure!(
						available_after <= available_before.get(&tick).copied().unwrap_or_default(),
						"A price is offering more liquidity than it was before the migration"
					);
				}
			}
		}

		Ok(())
	}
}

/// The pallet's index of open limit orders, as implied by the orders the pool actually holds.
fn build_limit_orders_cache<T: Config>(
	pool_state: &cf_amm::PoolState<(T::AccountId, OrderId)>,
) -> LimitOrdersCache<T> {
	let mut cache = LimitOrdersCache::<T>::default();

	for side in [Side::Sell, Side::Buy] {
		for ((account_id, order_id), tick, _position_info) in pool_state.limit_orders(side) {
			cache[side.to_sold_pair()].entry(account_id).or_default().insert(order_id, tick);
		}
	}

	cache
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, RuntimeOrigin, Test, ALICE, BOB};
	use cf_amm::{
		common::AssetPair,
		limit_orders::migration_support::{FixedPool, FloatBetweenZeroAndOne, PositionV9},
		math::{Price, SqrtPrice},
	};
	use cf_primitives::{Asset, STABLE_ASSET};
	use cf_traits::mocks::{balance_api::MockBalance, lp_stats_api::MockLpStatsApi};
	use cf_utilities::assert_err;
	use frame_support::assert_ok;
	use sp_runtime::{traits::Zero, FixedU128};

	const BASE_ASSET: Asset = Asset::Usdt;

	/// Order ids, which only have to be distinct per lp.
	const ALICE_ORDER: OrderId = 1;
	const BOB_ORDER: OrderId = 2;

	type AccountId = <Test as frame_system::Config>::AccountId;

	/// A price nothing has been bought from.
	fn untouched() -> FloatBetweenZeroAndOne {
		FloatBetweenZeroAndOne::one()
	}

	/// A price that swaps have taken these fractions of, folded into the running product one at a
	/// time exactly as the swaps would have.
	fn remaining_after(
		fractions: impl IntoIterator<Item = (u128, u128)>,
	) -> FloatBetweenZeroAndOne {
		fractions.into_iter().fold(untouched(), |remaining, (numerator, denominator)| {
			remaining.mul_div_ceil(numerator.into(), denominator.into())
		})
	}

	/// All the liquidity offered at one price, mirroring `FixedPool` field for field, plus the
	/// price it was stored under.
	struct OldFixedPool {
		/// The map key rather than part of the pool itself.
		tick: Tick,
		/// Identifies this pool among every fixed pool the pair ever had — one counter served all
		/// prices and both sides, and numbers were never reused. A price bought out was deleted
		/// and got a fresh number if liquidity returned, so an order recording a different one
		/// belongs to a pool that no longer exists and was therefore bought in full.
		pool_instance: u128,
		/// The liquidity still on offer here.
		available: Amount,
		/// The fraction of this price's liquidity still on offer.
		percent_remaining: FloatBetweenZeroAndOne,
	}

	/// A single order, mirroring `PositionV9` field for field, plus the key it was stored under.
	struct OldOrder {
		/// The price and lp are the map key rather than part of the order itself.
		tick: Tick,
		lp: (AccountId, OrderId),
		pool_instance: u128,
		/// The order's size when it was last touched.
		amount: Amount,
		/// The fraction the price had left when the order was last touched. The gap between this
		/// and the price's current value is how the old representation recovered the amount
		/// bought.
		last_percent_remaining: FloatBetweenZeroAndOne,
		/// The size as of the last mint or burn, which the migration must carry across unchanged.
		/// Differs from `amount` for any order a swap has partially bought.
		original_amount: Amount,
	}

	/// Replaces a real pool's limit orders with state in the pre-migration shape.
	fn put_old_pool(fixed_pools: Vec<OldFixedPool>, positions: Vec<OldOrder>) -> AssetPair {
		let asset_pair = AssetPair::new(BASE_ASSET, STABLE_ASSET).unwrap();

		assert_ok!(crate::Pallet::<Test>::new_pool(
			RuntimeOrigin::root(),
			BASE_ASSET,
			STABLE_ASSET,
			0,
			Price::at_tick_zero(),
		));
		let range_orders = Pools::<Test>::get(asset_pair).unwrap().pool_state.range_orders;

		let old_pool = old::Pool::<Test> {
			range_orders_cache: Default::default(),
			// Deliberately stale: the migration must rebuild this from the orders that survive
			// rather than carry it over.
			limit_orders_cache: Default::default(),
			pool_state: old::PoolState {
				limit_orders: PoolStateV9::from_parts(
					PoolPairsMap::from_array([
						fixed_pools
							.into_iter()
							.map(|fixed_pool| {
								(
									SqrtPrice::from_tick(fixed_pool.tick),
									FixedPool::from_parts(
										fixed_pool.pool_instance,
										fixed_pool.available,
										fixed_pool.percent_remaining,
									),
								)
							})
							.collect(),
						Default::default(),
					]),
					PoolPairsMap::from_array([
						positions
							.into_iter()
							.map(|order| {
								(
									(SqrtPrice::from_tick(order.tick), order.lp),
									PositionV9::from_parts(
										order.pool_instance,
										order.amount,
										order.last_percent_remaining,
										order.original_amount,
									),
								)
							})
							.collect(),
						Default::default(),
					]),
				),
				range_orders,
			},
		};

		old::Pools::<Test>::insert(asset_pair, old_pool);

		asset_pair
	}

	#[test]
	fn pays_out_what_orders_had_earned_and_reshapes_the_rest() {
		new_test_ext().execute_with(|| {
			let asset_pair = put_old_pool(
				vec![OldFixedPool {
					tick: 0,
					pool_instance: 0,
					available: 500.into(),
					percent_remaining: remaining_after([(2, 5), (1, 2)]),
				}],
				vec![
					// Alice minted 2_500 here. Swaps took three fifths of the price and she swept,
					// which left her holding 1_000 with her original still recorded as 2_500 and
					// her snapshot of the price at two fifths. Swaps have since taken half of what
					// was left, so her order is worth 500 — all the price still has on offer.
					OldOrder {
						tick: 0,
						lp: (ALICE, ALICE_ORDER),
						pool_instance: 0,
						amount: 1000.into(),
						last_percent_remaining: remaining_after([(2, 5)]),
						original_amount: 2500.into(),
					},
					// Bob's order is full bought because no fixed pool exists.
					OldOrder {
						tick: 120,
						lp: (BOB, BOB_ORDER),
						pool_instance: 1,
						amount: 800.into(),
						last_percent_remaining: untouched(),
						original_amount: 800.into(),
					},
				],
			);

			assert_eq!(MockBalance::get_balance(&ALICE, STABLE_ASSET), 0);
			assert_eq!(MockBalance::get_balance(&BOB, STABLE_ASSET), 0);

			Migration::<Test>::on_runtime_upgrade();

			let pool = Pools::<Test>::get(asset_pair).unwrap();

			// Tick zero is a price of one, so Alice is owed the 500 that was bought from her, and
			// keeps the half that was not.
			assert_eq!(MockBalance::get_balance(&ALICE, STABLE_ASSET), 500);
			let alice = pool.pool_state.limit_order(&(ALICE, ALICE_ORDER), Side::Sell, 0).unwrap();
			assert_eq!(alice.amount, 500.into());
			assert_eq!(alice.original_amount, 2500.into(),);

			// Bob is paid for all of his, and the order itself is gone. Tick 120 prices above one.
			// Bob's order sold all 800 of it at tick 120, a price of 1.0001^120 ≈ 1.01207, so he is
			// owed 809.66 rounded down.
			assert_eq!(MockBalance::get_balance(&BOB, STABLE_ASSET), 809);
			assert!(pool.pool_state.limit_order(&(BOB, BOB_ORDER), Side::Sell, 120).is_err());

			// The index is rebuilt from what survived, so it holds Alice's order and not Bob's.
			assert_eq!(pool.limit_orders_cache.base[&ALICE][&ALICE_ORDER], 0);
			assert!(!pool.limit_orders_cache.base.contains_key(&BOB));
		});
	}

	/// Sweeping used to record the volume of what it collected, and these proceeds would have been
	/// swept eventually. Settling them here without recording it would lose that volume for good.
	#[test]
	fn records_the_volume_it_settles() {
		new_test_ext().execute_with(|| {
			put_old_pool(
				vec![OldFixedPool {
					tick: 0,
					pool_instance: 0,
					available: 500.into(),
					percent_remaining: remaining_after([(1, 2)]),
				}],
				vec![OldOrder {
					tick: 0,
					lp: (ALICE, ALICE_ORDER),
					pool_instance: 0,
					amount: 1000.into(),
					last_percent_remaining: untouched(),
					original_amount: 1000.into(),
				}],
			);

			assert_eq!(MockLpStatsApi::delta_usd_volume(&ALICE, &STABLE_ASSET), Zero::zero());

			Migration::<Test>::on_runtime_upgrade();

			// The order sold ASSET for STABLE_ASSET, so the proceeds are the stable asset and the
			// volume is what was credited.
			assert_eq!(
				MockLpStatsApi::delta_usd_volume(&ALICE, &STABLE_ASSET),
				FixedU128::from_inner(500)
			);
		});
	}

	/// The other tests drop an order from a price that disappears with it. Here the price outlives
	/// the order, so the index has to lose one entry and keep its neighbour rather than being
	/// pruned wholesale.
	#[test]
	fn an_order_dropped_from_a_surviving_price_is_pruned_from_the_index() {
		const TICK: Tick = 120;

		new_test_ext().execute_with(|| {
			let asset_pair = put_old_pool(
				vec![OldFixedPool {
					tick: TICK,
					pool_instance: 5,
					available: 800.into(),
					percent_remaining: untouched(),
				}],
				vec![
					// Minted into an earlier pool at this price, which was emptied and deleted.
					// Every unit of it was bought, so it does not survive.
					OldOrder {
						tick: TICK,
						lp: (ALICE, ALICE_ORDER),
						pool_instance: 0,
						amount: 1000.into(),
						last_percent_remaining: untouched(),
						original_amount: 1000.into(),
					},
					// Minted into the pool that is still there, and nothing has been bought from
					// it since.
					OldOrder {
						tick: TICK,
						lp: (BOB, BOB_ORDER),
						pool_instance: 5,
						amount: 800.into(),
						last_percent_remaining: untouched(),
						original_amount: 800.into(),
					},
				],
			);

			Migration::<Test>::on_runtime_upgrade();

			let pool = Pools::<Test>::get(asset_pair).unwrap();

			// Sold all 1000 at a price of 1.0001^120, so 1012 rounded down.
			assert_eq!(MockBalance::get_balance(&ALICE, STABLE_ASSET), 1012);
			assert_err!(pool.pool_state.limit_order(&(ALICE, ALICE_ORDER), Side::Sell, TICK));
			assert!(!pool.limit_orders_cache.base.contains_key(&ALICE));

			// The price still has Bob's order on it, indexed as before.
			assert_eq!(
				pool.pool_state.limit_order(&(BOB, BOB_ORDER), Side::Sell, TICK).unwrap().amount,
				800.into()
			);
			assert_eq!(pool.limit_orders_cache.base[&BOB][&BOB_ORDER], TICK);
		});
	}

	#[test]
	fn removes_the_auto_sweeping_thresholds() {
		new_test_ext().execute_with(|| {
			// Written through the alias, so a value type that disagreed with the pallet's would
			// show up here rather than silently decoding into nonsense later.
			old::LimitOrderAutoSweepingThresholds::<Test>::put(
				BoundedBTreeMap::try_from(BTreeMap::from_iter([(Asset::Usdc, 1_000u128)])).unwrap(),
			);
			assert!(old::LimitOrderAutoSweepingThresholds::<Test>::exists());

			Migration::<Test>::on_runtime_upgrade();

			assert!(!old::LimitOrderAutoSweepingThresholds::<Test>::exists());
		});
	}

	#[test]
	fn an_empty_pool_migrates_cleanly() {
		new_test_ext().execute_with(|| {
			let asset_pair = put_old_pool(vec![], vec![]);

			Migration::<Test>::on_runtime_upgrade();

			let pool = Pools::<Test>::get(asset_pair).unwrap();
			assert!(pool.pool_state.limit_order_liquidity(Side::Sell).is_empty());
			assert!(pool.limit_orders_cache.base.is_empty());
		});
	}
}
