use cf_primitives::*;
use sp_core::{
	serde::{Deserialize, Serialize},
	U256,
};
use std::ops::Range;

use anyhow::anyhow;
use cf_primitives::chains::{assets::any, Bitcoin, Ethereum, Polkadot};
use cf_utilities::rpc::NumberOrHex;

use pallet_cf_pools::{CloseOrder, IncreaseOrDecrease, OrderId, RangeOrderSize};

use crate::SwapChannelInfo;
pub use cf_amm::{
	common::{PoolPairsMap, Side},
	math::Tick,
};

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

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct OrderIdJson(NumberOrHex);
impl TryFrom<OrderIdJson> for OrderId {
	type Error = anyhow::Error;

	fn try_from(value: OrderIdJson) -> Result<Self, Self::Error> {
		value.0.try_into().map_err(|_| anyhow!("Failed to convert order id to u64"))
	}
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum RangeOrderSizeJson {
	AssetAmounts { maximum: PoolPairsMap<NumberOrHex>, minimum: PoolPairsMap<NumberOrHex> },
	Liquidity { liquidity: NumberOrHex },
}
impl TryFrom<RangeOrderSizeJson> for RangeOrderSize {
	type Error = anyhow::Error;

	fn try_from(value: RangeOrderSizeJson) -> Result<Self, Self::Error> {
		Ok(match value {
			RangeOrderSizeJson::AssetAmounts { maximum, minimum } => RangeOrderSize::AssetAmounts {
				maximum: maximum
					.try_map(TryInto::try_into)
					.map_err(|_| anyhow!("Failed to convert maximums to u128"))?,
				minimum: minimum
					.try_map(TryInto::try_into)
					.map_err(|_| anyhow!("Failed to convert minimums to u128"))?,
			},
			RangeOrderSizeJson::Liquidity { liquidity } => RangeOrderSize::Liquidity {
				liquidity: liquidity
					.try_into()
					.map_err(|_| anyhow!("Failed to convert liquidity to u128"))?,
			},
		})
	}
}

#[derive(Serialize, Deserialize, Clone)]
pub struct OpenSwapChannels {
	pub ethereum: Vec<SwapChannelInfo<Ethereum>>,
	pub bitcoin: Vec<SwapChannelInfo<Bitcoin>>,
	pub polkadot: Vec<SwapChannelInfo<Polkadot>>,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CloseOrderJson {
	Limit { base_asset: any::Asset, quote_asset: any::Asset, side: Side, id: OrderIdJson },
	Range { base_asset: any::Asset, quote_asset: any::Asset, id: OrderIdJson },
}

impl TryFrom<CloseOrderJson> for CloseOrder {
	type Error = anyhow::Error;

	fn try_from(value: CloseOrderJson) -> Result<Self, Self::Error> {
		Ok(match value {
			CloseOrderJson::Limit { base_asset, quote_asset, side, id } =>
				CloseOrder::Limit { base_asset, quote_asset, side, id: id.try_into()? },
			CloseOrderJson::Range { base_asset, quote_asset, id } =>
				CloseOrder::Range { base_asset, quote_asset, id: id.try_into()? },
		})
	}
}
