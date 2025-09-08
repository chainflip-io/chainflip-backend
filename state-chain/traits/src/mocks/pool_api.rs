use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

use cf_amm::{
	common::{PoolPairsMap, Side},
	math::Tick,
};
use cf_chains::assets::any::AssetMap;
use cf_primitives::{Asset, AssetAmount, STABLE_ASSET};
use codec::{Decode, Encode};
use frame_support::{
	sp_runtime::{DispatchError, DispatchResult},
	weights::Weight,
};
use scale_info::TypeInfo;

use crate::{
	mocks::balance_api::MockBalance, BalanceApi, IncreaseOrDecrease, LpOrdersWeightsProvider,
	OrderId, PoolApi,
};

use super::{MockPallet, MockPalletStorage};

pub struct MockPoolApi {}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
struct TickAndAmount {
	tick: Tick,
	amount: AssetAmount,
}

#[derive(Debug, PartialEq, Eq)]
pub struct MockLimitOrder {
	pub base_asset: Asset,
	pub account_id: AccountId,
	pub side: Side,
	pub order_id: OrderId,
	pub tick: Tick,
	pub amount: AssetAmount,
}

impl MockPallet for MockPoolApi {
	const PREFIX: &'static [u8] = b"MockPoolApi";
}

const LIMIT_ORDERS: &[u8] = b"LIMIT_ORDERS";

impl MockPoolApi {
	pub fn get_limit_orders() -> Vec<MockLimitOrder> {
		Self::get_value::<LimitOrderStorage>(LIMIT_ORDERS)
			.unwrap_or_default()
			.into_iter()
			.map(
				|(
					MockLimitOrderStorageKey { base_asset, account_id, side, order_id },
					TickAndAmount { tick, amount },
				)| { MockLimitOrder { base_asset, account_id, side, order_id, tick, amount } },
			)
			.collect()
	}
}

type AccountId = u64;

#[derive(Clone, Debug, Encode, Decode, PartialOrd, Ord, PartialEq, Eq)]
pub struct MockLimitOrderStorageKey {
	pub base_asset: Asset,
	pub account_id: AccountId,
	pub side: Side,
	pub order_id: OrderId,
}

type LimitOrderStorage = BTreeMap<MockLimitOrderStorageKey, TickAndAmount>;

impl PoolApi for MockPoolApi {
	type AccountId = AccountId;

	fn sweep(_who: &Self::AccountId) -> Result<(), DispatchError> {
		Ok(())
	}

	fn open_order_count(
		who: &Self::AccountId,
		asset_pair: &PoolPairsMap<Asset>,
	) -> Result<u32, DispatchError> {
		let limit_orders = Self::get_value::<LimitOrderStorage>(LIMIT_ORDERS).unwrap_or_default();
		let count = limit_orders
			.keys()
			.filter(|MockLimitOrderStorageKey { base_asset, account_id, .. }| {
				account_id == who && asset_pair.base == *base_asset
			})
			.count() as u32;
		Ok(count)
	}

	fn open_order_balances(who: &Self::AccountId) -> AssetMap<AssetAmount> {
		AssetMap::from_fn(|asset| {
			Self::get_value::<LimitOrderStorage>(LIMIT_ORDERS)
				.unwrap_or_default()
				.into_iter()
				.filter_map(
					|(
						MockLimitOrderStorageKey { base_asset, account_id, side, .. },
						TickAndAmount { tick: _, amount },
					)| {
						if account_id == *who &&
							((asset == base_asset && side == Side::Sell) ||
								(asset == STABLE_ASSET && side == Side::Buy))
						{
							Some(amount)
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
		Self::mutate_value(LIMIT_ORDERS, |limit_orders: &mut Option<LimitOrderStorage>| {
			if let Some(limit_orders) = limit_orders {
				limit_orders.retain(
					|MockLimitOrderStorageKey { base_asset, account_id, side, .. }, tick_amount| {
						if account_id == who {
							MockBalance::credit_account(
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
		})
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

		Self::mutate_value(LIMIT_ORDERS, |limit_orders: &mut Option<LimitOrderStorage>| {
			let limit_orders = limit_orders.get_or_insert_default();

			let key =
				MockLimitOrderStorageKey { base_asset, account_id: *account, side, order_id: id };
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
					MockBalance::try_debit_account(account, sold_asset, amount).unwrap(),
				IncreaseOrDecrease::Decrease(amount) =>
					MockBalance::credit_account(account, sold_asset, amount),
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
		});

		Ok(())
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn create_pool(
		_base_asset: Asset,
		_quote_asset: Asset,
		_fee_hundredth_pips: u32,
		_initial_price: cf_primitives::Price,
	) -> DispatchResult {
		unimplemented!()
	}
}

impl LpOrdersWeightsProvider for MockPoolApi {
	fn update_limit_order_weight() -> Weight {
		Weight::zero()
	}
}
