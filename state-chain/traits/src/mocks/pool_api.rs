#[cfg(feature = "runtime-benchmarks")]
use cf_amm::math::Price;
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	marker::PhantomData,
	vec::Vec,
};

use cf_amm::{
	common::{LimitOrder, PoolPairsMap, Side},
	math::Tick,
};
use cf_chains::assets::any::AssetMap;
use cf_primitives::{Asset, AssetAmount, OrderId, STABLE_ASSET};
use codec::{Decode, Encode};
use frame_support::{
	sp_runtime::{DispatchError, DispatchResult},
	weights::Weight,
};
use scale_info::TypeInfo;

use crate::{
	mocks::balance_api::MockBalance, BalanceApi, IncreaseOrDecrease, LpOrdersWeightsProvider,
	PoolApi,
};

use super::{MockPallet, MockPalletStorage};

pub struct MockPoolApi<AccountId = u64>(PhantomData<AccountId>);

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
struct TickAndAmount {
	tick: Tick,
	amount: AssetAmount,
}

impl<AccountId> MockPallet for MockPoolApi<AccountId> {
	const PREFIX: &'static [u8] = b"MockPoolApi";
}

const LIMIT_ORDERS: &[u8] = b"LIMIT_ORDERS";

impl<AccountId> MockPoolApi<AccountId>
where
	AccountId: Decode + Encode + Ord,
{
	pub fn get_limit_orders() -> Vec<LimitOrder<AccountId>> {
		Self::get_value::<LimitOrderStorage<AccountId>>(LIMIT_ORDERS)
			.unwrap_or_default()
			.into_iter()
			.map(
				|(
					MockLimitOrderStorageKey { base_asset, account_id, side, order_id },
					TickAndAmount { tick, amount },
				)| {
					LimitOrder {
						base_asset,
						quote_asset: STABLE_ASSET,
						account_id,
						side,
						order_id,
						tick,
						amount,
					}
				},
			)
			.collect()
	}
}

#[derive(Clone, Debug, Encode, Decode, PartialOrd, Ord, PartialEq, Eq)]
pub struct MockLimitOrderStorageKey<AccountId = u64> {
	pub base_asset: Asset,
	pub account_id: AccountId,
	pub side: Side,
	pub order_id: OrderId,
}

type LimitOrderStorage<AccountId> = BTreeMap<MockLimitOrderStorageKey<AccountId>, TickAndAmount>;

impl<AccountId> PoolApi for MockPoolApi<AccountId>
where
	AccountId: Clone + Decode + Encode + Ord,
{
	type AccountId = AccountId;

	fn sweep(_who: &Self::AccountId) -> Result<(), DispatchError> {
		Ok(())
	}

	fn open_order_count(
		who: &Self::AccountId,
		asset_pair: &PoolPairsMap<Asset>,
	) -> Result<u32, DispatchError> {
		let limit_orders =
			Self::get_value::<LimitOrderStorage<AccountId>>(LIMIT_ORDERS).unwrap_or_default();
		let count = limit_orders
			.keys()
			.filter(|MockLimitOrderStorageKey { base_asset, account_id, .. }| {
				account_id == who && asset_pair.base == *base_asset
			})
			.count() as u32;
		Ok(count)
	}

	fn limit_orders(
		base_asset: Asset,
		_quote_asset: Asset,
		accounts: &BTreeSet<Self::AccountId>,
	) -> Result<Vec<LimitOrder<Self::AccountId>>, DispatchError> {
		Ok(Self::get_value::<LimitOrderStorage<AccountId>>(LIMIT_ORDERS)
			.unwrap_or_default()
			.into_iter()
			.map(
				|(
					MockLimitOrderStorageKey { base_asset, account_id, side, order_id },
					TickAndAmount { tick, amount },
				)| {
					LimitOrder {
						base_asset,
						quote_asset: STABLE_ASSET,
						account_id,
						side,
						order_id,
						tick,
						amount,
					}
				},
			)
			.filter(|order| order.base_asset == base_asset && accounts.contains(&order.account_id))
			.collect())
	}

	fn open_order_balances(who: &Self::AccountId) -> AssetMap<AssetAmount> {
		AssetMap::from_fn(|asset| {
			let limit_orders =
				Self::get_value::<LimitOrderStorage<AccountId>>(LIMIT_ORDERS).unwrap_or_default();
			limit_orders
				.iter()
				.filter_map(
					|(
						MockLimitOrderStorageKey { base_asset, account_id, side, .. },
						TickAndAmount { tick: _, amount },
					)| {
						if account_id == who &&
							((asset == *base_asset && *side == Side::Sell) ||
								(asset == STABLE_ASSET && *side == Side::Buy))
						{
							Some(*amount)
						} else {
							None
						}
					},
				)
				.sum()
		})
	}

	fn pools() -> Vec<PoolPairsMap<Asset>> {
		Asset::all()
			.filter_map(|asset| {
				if asset != STABLE_ASSET {
					Some(PoolPairsMap { base: asset, quote: STABLE_ASSET })
				} else {
					None
				}
			})
			.collect()
	}

	fn cancel_all_limit_orders(who: &Self::AccountId) -> frame_support::dispatch::DispatchResult {
		Self::mutate_value(
			LIMIT_ORDERS,
			|limit_orders: &mut Option<LimitOrderStorage<AccountId>>| {
				if let Some(limit_orders) = limit_orders {
					limit_orders.retain(
						|MockLimitOrderStorageKey { base_asset, account_id, side, .. },
						 tick_amount| {
							if account_id == who {
								MockBalance::<AccountId>::credit_account(
									account_id,
									if *side == Side::Sell { *base_asset } else { STABLE_ASSET },
									tick_amount.amount,
								);
								return false;
							}
							true
						},
					);
				}
				Ok(())
			},
		)
	}

	fn update_limit_order(
		account: &Self::AccountId,
		base_asset: Asset,
		quote_asset: Asset,
		side: Side,
		id: OrderId,
		option_tick: Option<cf_primitives::Tick>,
		amount_change: IncreaseOrDecrease<AssetAmount>,
	) -> DispatchResult {
		assert_eq!(quote_asset, STABLE_ASSET);

		Self::mutate_value(
			LIMIT_ORDERS,
			|limit_orders: &mut Option<LimitOrderStorage<AccountId>>| {
				let limit_orders = limit_orders.get_or_insert_default();

				let key = MockLimitOrderStorageKey {
					base_asset,
					account_id: account.clone(),
					side,
					order_id: id,
				};
				let amount_change = match amount_change {
					IncreaseOrDecrease::Increase(_) => amount_change,
					// Support for cancel order decreasing by u128::MAX
					IncreaseOrDecrease::Decrease(amount) => {
						let max_amount = limit_orders.get(&key).unwrap().amount;
						IncreaseOrDecrease::Decrease(amount.min(max_amount))
					},
				};

				let order = limit_orders.remove(&key);

				// Handle balance changes
				let sold_asset = if side == Side::Buy { quote_asset } else { base_asset };
				match amount_change {
					IncreaseOrDecrease::Increase(amount) =>
						MockBalance::<AccountId>::try_debit_account(account, sold_asset, amount)
							.unwrap(),
					IncreaseOrDecrease::Decrease(amount) =>
						MockBalance::<AccountId>::credit_account(account, sold_asset, amount),
				};

				let maybe_order = match order {
					None => {
						// Creating new order if none exists
						let tick = option_tick
							.expect("Tick must be provided for an order that does not exist");
						let amount = match amount_change {
							IncreaseOrDecrease::Increase(amount) => amount,
							IncreaseOrDecrease::Decrease(_) =>
								panic!("cannot decrease amount if order does not exist"),
						};

						Some(TickAndAmount { tick, amount })
					},
					Some(mut order) => {
						if let Some(tick) = option_tick {
							order.tick = tick;
						}

						match amount_change {
							IncreaseOrDecrease::Increase(amount) => {
								order.amount += amount;
								Some(order)
							},
							IncreaseOrDecrease::Decrease(amount) =>
								if order.amount <= amount {
									// Reduced to 0, so close the order
									None
								} else {
									order.amount -= amount;
									Some(order)
								},
						}
					},
				};

				if let Some(order) = maybe_order {
					limit_orders.insert(key, order);
				}
			},
		);

		Ok(())
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn create_pool(
		_base_asset: Asset,
		_quote_asset: Asset,
		_fee_hundredth_pips: u32,
		_initial_price: Price,
	) -> DispatchResult {
		unimplemented!()
	}
}

impl<AccountId> LpOrdersWeightsProvider for MockPoolApi<AccountId> {
	fn update_limit_order_weight() -> Weight {
		Weight::zero()
	}
}
