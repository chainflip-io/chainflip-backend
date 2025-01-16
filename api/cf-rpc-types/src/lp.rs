pub use cf_amm::{
	common::{PoolPairsMap, Side},
	math::{Amount, Tick},
	range_orders::Liquidity,
};
use cf_primitives::Asset;
use frame_support::{Deserialize, Serialize};
use sp_core::{H256, U256};
use std::ops::Range;

use pallet_cf_pools::IncreaseOrDecrease;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ApiWaitForResult<T> {
	TxHash(H256),
	TxDetails { tx_hash: H256, response: T },
}

impl<T> ApiWaitForResult<T> {
	pub fn map_details<R>(self, f: impl FnOnce(T) -> R) -> ApiWaitForResult<R> {
		match self {
			ApiWaitForResult::TxHash(hash) => ApiWaitForResult::TxHash(hash),
			ApiWaitForResult::TxDetails { response, tx_hash } =>
				ApiWaitForResult::TxDetails { tx_hash, response: f(response) },
		}
	}

	#[track_caller]
	pub fn unwrap_details(self) -> T {
		match self {
			ApiWaitForResult::TxHash(_) => panic!("unwrap_details called on TransactionHash"),
			ApiWaitForResult::TxDetails { response, .. } => response,
		}
	}
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RangeOrder {
	pub base_asset: Asset,
	pub quote_asset: Asset,
	pub id: U256,
	pub tick_range: Range<Tick>,
	pub liquidity_total: U256,
	pub collected_fees: PoolPairsMap<U256>,
	pub size_change: Option<IncreaseOrDecrease<RangeOrderChange>>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RangeOrderChange {
	pub liquidity: U256,
	pub amounts: PoolPairsMap<U256>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LimitOrder {
	pub base_asset: Asset,
	pub quote_asset: Asset,
	pub side: Side,
	pub id: U256,
	pub tick: Tick,
	pub sell_amount_total: U256,
	pub collected_fees: U256,
	pub bought_amount: U256,
	pub sell_amount_change: Option<IncreaseOrDecrease<U256>>,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum LimitOrRangeOrder {
	LimitOrder(LimitOrder),
	RangeOrder(RangeOrder),
}

pub fn collect_range_order_returns(
	events: impl IntoIterator<Item = state_chain_runtime::RuntimeEvent>,
) -> Vec<RangeOrder> {
	filter_orders(events)
		.filter_map(|order| match order {
			LimitOrRangeOrder::RangeOrder(range_order) => Some(range_order),
			_ => None,
		})
		.collect()
}

pub fn collect_limit_order_returns(
	events: impl IntoIterator<Item = state_chain_runtime::RuntimeEvent>,
) -> Vec<LimitOrder> {
	filter_orders(events)
		.filter_map(|order| match order {
			LimitOrRangeOrder::LimitOrder(limit_order) => Some(limit_order),
			_ => None,
		})
		.collect()
}

pub fn collect_order_returns(
	events: impl IntoIterator<Item = state_chain_runtime::RuntimeEvent>,
) -> Vec<LimitOrRangeOrder> {
	filter_orders(events).collect()
}

pub fn filter_orders(
	events: impl IntoIterator<Item = state_chain_runtime::RuntimeEvent>,
) -> impl Iterator<Item = LimitOrRangeOrder> {
	events.into_iter().filter_map(|event| match event {
		state_chain_runtime::RuntimeEvent::LiquidityPools(
			pallet_cf_pools::Event::LimitOrderUpdated {
				sell_amount_change,
				sell_amount_total,
				collected_fees,
				bought_amount,
				tick,
				base_asset,
				quote_asset,
				side,
				id,
				..
			},
		) => Some(LimitOrRangeOrder::LimitOrder(LimitOrder {
			base_asset,
			quote_asset,
			side,
			id: id.into(),
			tick,
			sell_amount_total: sell_amount_total.into(),
			collected_fees: collected_fees.into(),
			bought_amount: bought_amount.into(),
			sell_amount_change: sell_amount_change
				.map(|increase_or_decrease| increase_or_decrease.map(|amount| amount.into())),
		})),
		state_chain_runtime::RuntimeEvent::LiquidityPools(
			pallet_cf_pools::Event::RangeOrderUpdated {
				size_change,
				liquidity_total,
				collected_fees,
				tick_range,
				base_asset,
				quote_asset,
				id,
				..
			},
		) => Some(LimitOrRangeOrder::RangeOrder(RangeOrder {
			base_asset,
			quote_asset,
			id: id.into(),
			size_change: size_change.map(|increase_or_decrease| {
				increase_or_decrease.map(|range_order_change| RangeOrderChange {
					liquidity: range_order_change.liquidity.into(),
					amounts: range_order_change.amounts.map(|amount| amount.into()),
				})
			}),
			liquidity_total: liquidity_total.into(),
			tick_range,
			collected_fees: collected_fees.map(Into::into),
		})),
		_ => None,
	})
}
