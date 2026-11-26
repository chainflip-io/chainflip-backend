// Copyright 2026 Chainflip Labs GmbH
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
use cf_amm::{
	common::{AssetPair, Side},
	limit_orders::Backing,
	math::{Price, Tick},
	range_orders::Liquidity,
};
use cf_primitives::OrderId;
use pallet_cf_pools::{
	AssetAmounts, CloseOrder, IncreaseOrDecrease, PalletConfigUpdate, RangeOrderChange,
};

/// `pallet_cf_pools::Event` before limit orders recorded their backing. Variants and fields must
/// stay exactly as they were: historical blocks are decoded with this type.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum PoolsEvent {
	NewPoolCreated {
		base_asset: Asset,
		quote_asset: Asset,
		fee_hundredth_pips: u32,
		initial_price: Price,
	},
	RangeOrderUpdated {
		lp: AccountId,
		base_asset: Asset,
		quote_asset: Asset,
		id: OrderId,
		tick_range: core::ops::Range<Tick>,
		size_change: Option<IncreaseOrDecrease<RangeOrderChange>>,
		liquidity_total: Liquidity,
		collected_fees: AssetAmounts,
	},
	LimitOrderUpdated {
		lp: AccountId,
		base_asset: Asset,
		quote_asset: Asset,
		side: Side,
		id: OrderId,
		tick: Tick,
		sell_amount_change: Option<IncreaseOrDecrease<AssetAmount>>,
		sell_amount_total: AssetAmount,
		collected_fees: AssetAmount,
		bought_amount: AssetAmount,
	},
	AssetSwapped {
		from: Asset,
		to: Asset,
		input_amount: AssetAmount,
		output_amount: AssetAmount,
	},
	PoolFeeSet {
		base_asset: Asset,
		quote_asset: Asset,
		fee_hundredth_pips: u32,
	},
	ScheduledLimitOrderUpdateDispatchSuccess {
		lp: AccountId,
		order_id: OrderId,
	},
	ScheduledLimitOrderUpdateDispatchFailure {
		lp: AccountId,
		order_id: OrderId,
		error: DispatchError,
	},
	LimitOrderSetOrUpdateScheduled {
		lp: AccountId,
		order_id: OrderId,
		dispatch_at: BlockNumber,
	},
	PriceImpactLimitSet {
		asset_pair: AssetPair,
		limit: Option<u32>,
	},
	OrderDeletionFailed {
		order: CloseOrder,
	},
	PalletConfigUpdated {
		update: PalletConfigUpdate,
	},
	LimitOrderFilled {
		lp: AccountId,
		base_asset: Asset,
		quote_asset: Asset,
		side: Side,
		id: OrderId,
		tick: Tick,
		sold_amount: AssetAmount,
		bought_amount: AssetAmount,
		remaining_amount: AssetAmount,
	},
}

impl From<PoolsEvent> for pallet_cf_pools::Event<Runtime> {
	fn from(event: PoolsEvent) -> Self {
		match event {
			PoolsEvent::NewPoolCreated {
				base_asset,
				quote_asset,
				fee_hundredth_pips,
				initial_price,
			} => Self::NewPoolCreated { base_asset, quote_asset, fee_hundredth_pips, initial_price },
			PoolsEvent::RangeOrderUpdated {
				lp,
				base_asset,
				quote_asset,
				id,
				tick_range,
				size_change,
				liquidity_total,
				collected_fees,
			} => Self::RangeOrderUpdated {
				lp,
				base_asset,
				quote_asset,
				id,
				tick_range,
				size_change,
				liquidity_total,
				collected_fees,
			},
			PoolsEvent::LimitOrderUpdated {
				lp,
				base_asset,
				quote_asset,
				side,
				id,
				tick,
				sell_amount_change,
				sell_amount_total,
				collected_fees,
				bought_amount,
			} => Self::LimitOrderUpdated {
				lp,
				base_asset,
				quote_asset,
				side,
				id,
				tick,
				sell_amount_change,
				sell_amount_total,
				collected_fees,
				bought_amount,
				// Every order predating backings was funded from the LP's balance.
				backing: Backing::Pool,
			},
			PoolsEvent::AssetSwapped { from, to, input_amount, output_amount } =>
				Self::AssetSwapped { from, to, input_amount, output_amount },
			PoolsEvent::PoolFeeSet { base_asset, quote_asset, fee_hundredth_pips } =>
				Self::PoolFeeSet { base_asset, quote_asset, fee_hundredth_pips },
			PoolsEvent::ScheduledLimitOrderUpdateDispatchSuccess { lp, order_id } =>
				Self::ScheduledLimitOrderUpdateDispatchSuccess { lp, order_id },
			PoolsEvent::ScheduledLimitOrderUpdateDispatchFailure { lp, order_id, error } =>
				Self::ScheduledLimitOrderUpdateDispatchFailure { lp, order_id, error },
			PoolsEvent::LimitOrderSetOrUpdateScheduled { lp, order_id, dispatch_at } =>
				Self::LimitOrderSetOrUpdateScheduled { lp, order_id, dispatch_at },
			PoolsEvent::PriceImpactLimitSet { asset_pair, limit } =>
				Self::PriceImpactLimitSet { asset_pair, limit },
			PoolsEvent::OrderDeletionFailed { order } => Self::OrderDeletionFailed { order },
			PoolsEvent::PalletConfigUpdated { update } => Self::PalletConfigUpdated { update },
			PoolsEvent::LimitOrderFilled {
				lp,
				base_asset,
				quote_asset,
				side,
				id,
				tick,
				sold_amount,
				bought_amount,
				remaining_amount,
			} => Self::LimitOrderFilled {
				lp,
				base_asset,
				quote_asset,
				side,
				id,
				tick,
				sold_amount,
				bought_amount,
				remaining_amount,
			},
		}
	}
}
