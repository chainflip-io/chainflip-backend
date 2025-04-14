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

use anyhow::anyhow;
use cf_chains::{address::AddressString, AnyChain, Arbitrum, Chain, Solana};
use cf_primitives::{
	chains::{assets::any, Bitcoin, Ethereum, Polkadot},
	*,
};
use cf_utilities::rpc::NumberOrHex;
use pallet_cf_pools::{OrderId, RangeOrderSize};
use sp_core::serde::{Deserialize, Serialize};
use std::ops::Range;

use crate::{SwapChannelInfo, U256};

pub use cf_amm::{
	common::{PoolPairsMap, Side},
	math::Tick,
};
pub use pallet_cf_pools::{CloseOrder, IncreaseOrDecrease, MAX_ORDERS_DELETE};

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
	pub arbitrum: Vec<SwapChannelInfo<Arbitrum>>,
	pub solana: Vec<SwapChannelInfo<Solana>>,
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

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct SwapRequestResponse {
	pub swap_request_id: SwapRequestId,
}

impl From<SwapRequestId> for SwapRequestResponse {
	fn from(swap_request_id: SwapRequestId) -> Self {
		SwapRequestResponse { swap_request_id }
	}
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LiquidityDepositChannelDetails {
	pub deposit_address: AddressString,
	pub deposit_chain_expiry_block: <AnyChain as Chain>::ChainBlockNumber,
}
