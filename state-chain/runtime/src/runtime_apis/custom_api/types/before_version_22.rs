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

use super::*;

// TradingStrategy before the `TickZeroCentered` and `SimpleBuySell` strategies were removed.
#[derive(Encode, Decode, TypeInfo, Clone)]
pub enum TradingStrategy {
	TickZeroCentered {
		spread_tick: Tick,
		base_asset: Asset,
	},
	SimpleBuySell {
		buy_tick: Tick,
		sell_tick: Tick,
		base_asset: Asset,
	},
	InventoryBased {
		min_buy_tick: Tick,
		max_buy_tick: Tick,
		min_sell_tick: Tick,
		max_sell_tick: Tick,
		base_asset: Asset,
	},
	OracleTracking {
		min_buy_offset_tick: Tick,
		max_buy_offset_tick: Tick,
		min_sell_offset_tick: Tick,
		max_sell_offset_tick: Tick,
		base_asset: Asset,
		quote_asset: Asset,
	},
}

impl TradingStrategy {
	/// Returns `None` for the removed strategy types.
	fn into_current(self) -> Option<pallet_cf_trading_strategy::TradingStrategy> {
		use pallet_cf_trading_strategy::TradingStrategy as Current;
		match self {
			TradingStrategy::TickZeroCentered { .. } | TradingStrategy::SimpleBuySell { .. } =>
				None,
			TradingStrategy::InventoryBased {
				min_buy_tick,
				max_buy_tick,
				min_sell_tick,
				max_sell_tick,
				base_asset,
			} => Some(Current::InventoryBased {
				min_buy_tick,
				max_buy_tick,
				min_sell_tick,
				max_sell_tick,
				base_asset,
			}),
			TradingStrategy::OracleTracking {
				min_buy_offset_tick,
				max_buy_offset_tick,
				min_sell_offset_tick,
				max_sell_offset_tick,
				base_asset,
				quote_asset,
			} => Some(Current::OracleTracking {
				min_buy_offset_tick,
				max_buy_offset_tick,
				min_sell_offset_tick,
				max_sell_offset_tick,
				base_asset,
				quote_asset,
			}),
		}
	}
}

#[derive(Encode, Decode, TypeInfo, Clone)]
pub struct TradingStrategyInfo<Amount> {
	pub lp_id: AccountId32,
	pub strategy_id: AccountId32,
	pub strategy: TradingStrategy,
	pub balance: Vec<(Asset, Amount)>,
}

impl<Amount> TradingStrategyInfo<Amount> {
	/// Returns `None` for strategies of a removed type.
	pub fn into_current(self) -> Option<super::TradingStrategyInfo<Amount>> {
		Some(super::TradingStrategyInfo {
			lp_id: self.lp_id,
			strategy_id: self.strategy_id,
			strategy: self.strategy.into_current()?,
			balance: self.balance,
		})
	}
}
