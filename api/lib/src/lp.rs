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

use super::{
	extract_liquidity_deposit_channel_details, fetch_preallocated_channels, SimpleSubmissionApi,
};
use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
pub use cf_amm::{
	common::{PoolPairsMap, Side},
	math::{Amount, Tick},
	range_orders::Liquidity,
};
use cf_chains::{address::AddressString, ForeignChain};
use cf_node_client::WaitForResult;
use cf_primitives::{
	AccountId, ApiWaitForResult, Asset, AssetAmount, BasisPoints, BlockNumber, DcaParameters,
	EgressId, PriceLimits, SwapRequestId, WaitFor,
};
pub use cf_rpc_types::lp::{
	CloseOrderJson, LimitOrRangeOrder, LimitOrder, LiquidityDepositChannelDetails,
	OpenSwapChannels, OrderIdJson, RangeOrder, RangeOrderChange, RangeOrderSizeJson,
};
use cf_rpc_types::ExtrinsicResponse;
use chainflip_engine::state_chain_observer::client::{
	extrinsic_api::signed::{SignedExtrinsicApi, UntilFinalized, UntilInBlock},
	DefaultRpcClient, StateChainClient,
};
use frame_support::{pallet_prelude::ConstU32, BoundedVec};
use futures::{FutureExt, TryFutureExt};
use pallet_cf_pools::{CloseOrder, IncreaseOrDecrease, OrderId, RangeOrderSize, MAX_ORDERS_DELETE};
use sp_core::H256;
use state_chain_runtime::RuntimeCall;
use std::{ops::Range, sync::Arc};

fn collect_range_order_returns(
	events: impl IntoIterator<Item = state_chain_runtime::RuntimeEvent>,
) -> Vec<RangeOrder> {
	filter_orders(events)
		.filter_map(|order| match order {
			LimitOrRangeOrder::RangeOrder(range_order) => Some(range_order),
			_ => None,
		})
		.collect()
}

fn collect_limit_order_returns(
	events: impl IntoIterator<Item = state_chain_runtime::RuntimeEvent>,
) -> Vec<LimitOrder> {
	filter_orders(events)
		.filter_map(|order| match order {
			LimitOrRangeOrder::LimitOrder(limit_order) => Some(limit_order),
			_ => None,
		})
		.collect()
}

fn collect_order_returns(
	events: impl IntoIterator<Item = state_chain_runtime::RuntimeEvent>,
) -> Vec<LimitOrRangeOrder> {
	filter_orders(events).collect()
}

fn filter_orders(
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

impl LpApi for StateChainClient {
	fn base_rpc_client(&self) -> Arc<DefaultRpcClient> {
		self.base_rpc_client.clone()
	}
}

fn into_api_wait_for_result<T>(
	from: WaitForResult,
	map_events: impl FnOnce(Vec<state_chain_runtime::RuntimeEvent>) -> T,
) -> ApiWaitForResult<T> {
	match from {
		WaitForResult::TransactionHash(tx_hash) => ApiWaitForResult::TxHash(tx_hash),
		WaitForResult::Details(extrinsic_data) => ApiWaitForResult::TxDetails {
			tx_hash: extrinsic_data.tx_hash,
			response: map_events(extrinsic_data.events),
		},
	}
}

#[async_trait]
pub trait LpApi: SignedExtrinsicApi + Sized + Send + Sync + 'static {
	fn base_rpc_client(&self) -> Arc<DefaultRpcClient>;

	async fn register_liquidity_refund_address(
		&self,
		chain: ForeignChain,
		address: AddressString,
	) -> Result<H256> {
		Ok(self
			.submit_signed_extrinsic(RuntimeCall::from(
				pallet_cf_lp::Call::register_liquidity_refund_address {
					address: address
						.try_parse_to_encoded_address(chain)
						.map_err(anyhow::Error::msg)?,
				},
			))
			.await
			.until_in_block()
			.await?
			.tx_hash)
	}

	async fn request_liquidity_deposit_address(
		&self,
		asset: Asset,
		wait_for: WaitFor,
		boost_fee: Option<BasisPoints>,
	) -> Result<ApiWaitForResult<LiquidityDepositChannelDetails>> {
		let wait_for_result = self
			.submit_signed_extrinsic_wait_for(
				pallet_cf_lp::Call::request_liquidity_deposit_address {
					asset,
					boost_fee: boost_fee.unwrap_or_default(),
				},
				wait_for,
			)
			.await?;

		Ok(match wait_for_result {
			WaitForResult::TransactionHash(tx_hash) => return Ok(ApiWaitForResult::TxHash(tx_hash)),
			WaitForResult::Details(extrinsic_data) => {
				let (_, details) =
					extract_liquidity_deposit_channel_details(extrinsic_data.events)?;
				ApiWaitForResult::TxDetails { tx_hash: extrinsic_data.tx_hash, response: details }
			},
		})
	}

	async fn request_liquidity_deposit_address_v2(
		&self,
		asset: Asset,
		boost_fee: Option<BasisPoints>,
	) -> Result<ExtrinsicResponse<LiquidityDepositChannelDetails>> {
		let submit_signed_extrinsic_fut = self
			.submit_signed_extrinsic_with_dry_run(
				pallet_cf_lp::Call::request_liquidity_deposit_address {
					asset,
					boost_fee: boost_fee.unwrap_or_default(),
				},
			)
			.and_then(|(_, (block_fut, finalized_fut))| async move {
				let extrinsic_data = block_fut.until_in_block().await?;
				let (channel_id, details) =
					extract_liquidity_deposit_channel_details(extrinsic_data.events)?;
				Ok((
					channel_id,
					details,
					extrinsic_data.header,
					extrinsic_data.tx_index,
					extrinsic_data.block_hash,
					finalized_fut,
				))
			})
			.boxed();

		// Get the pre-allocated channels from the previous finalized block
		let preallocated_channels_fut =
			fetch_preallocated_channels(self.base_rpc_client(), self.account_id(), asset.into());

		let (
			(channel_id, details, header, tx_index, block_hash, finalized_fut),
			preallocated_channels,
		) = futures::try_join!(submit_signed_extrinsic_fut, preallocated_channels_fut)?;

		// If the extracted deposit channel was pre-allocated to this lp
		// in the previous finalized block, we can return it immediately.
		if preallocated_channels.contains(&channel_id) {
			return Ok(ExtrinsicResponse {
				response: details,
				tx_index,
				block_number: header.number,
				block_hash,
			});
		};

		// Worst case, we need to wait for the transaction to be finalized.
		let extrinsic_data = finalized_fut.until_finalized().await?;
		let (_channel_id, details) =
			extract_liquidity_deposit_channel_details(extrinsic_data.events)?;
		Ok(ExtrinsicResponse {
			response: details,
			tx_index: extrinsic_data.tx_index,
			block_number: extrinsic_data.header.number,
			block_hash: extrinsic_data.block_hash,
		})
	}

	async fn withdraw_asset(
		&self,
		amount: AssetAmount,
		asset: Asset,
		destination_address: AddressString,
		wait_for: WaitFor,
	) -> Result<ApiWaitForResult<EgressId>> {
		if amount == 0 {
			bail!("Withdrawal amount must be greater than 0");
		}

		let wait_for_result = self
			.submit_signed_extrinsic_wait_for(
				pallet_cf_lp::Call::withdraw_asset {
					amount,
					asset,
					destination_address: destination_address
						.try_parse_to_encoded_address(asset.into())
						.map_err(anyhow::Error::msg)?,
				},
				wait_for,
			)
			.await?;

		Ok(match wait_for_result {
			WaitForResult::TransactionHash(tx_hash) => return Ok(ApiWaitForResult::TxHash(tx_hash)),
			WaitForResult::Details(extrinsic_data) => {
				let egress_id = extrinsic_data
					.events
					.into_iter()
					.find_map(|event| match event {
						state_chain_runtime::RuntimeEvent::LiquidityProvider(
							pallet_cf_lp::Event::WithdrawalEgressScheduled { egress_id, .. },
						) => Some(egress_id),
						_ => None,
					})
					.ok_or_else(|| anyhow!("No WithdrawalEgressScheduled event was found"))?;

				ApiWaitForResult::TxDetails { tx_hash: extrinsic_data.tx_hash, response: egress_id }
			},
		})
	}

	async fn transfer_asset(
		&self,
		amount: AssetAmount,
		asset: Asset,
		destination: AccountId,
	) -> Result<H256> {
		if amount == 0 {
			bail!("Amount must be greater than 0");
		}
		Ok(self
			.submit_signed_extrinsic(RuntimeCall::from(pallet_cf_lp::Call::transfer_asset {
				amount,
				asset,
				destination,
			}))
			.await
			.until_in_block()
			.await?
			.tx_hash)
	}

	async fn update_range_order(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		id: OrderId,
		option_tick_range: Option<Range<Tick>>,
		size_change: IncreaseOrDecrease<RangeOrderSize>,
		wait_for: WaitFor,
	) -> Result<ApiWaitForResult<Vec<RangeOrder>>> {
		// Submit the mint order
		Ok(into_api_wait_for_result(
			self.submit_signed_extrinsic_wait_for(
				pallet_cf_pools::Call::update_range_order {
					base_asset,
					quote_asset,
					id,
					option_tick_range,
					size_change,
				},
				wait_for,
			)
			.await?,
			collect_range_order_returns,
		))
	}

	async fn set_range_order(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		id: OrderId,
		option_tick_range: Option<Range<Tick>>,
		size: RangeOrderSize,
		wait_for: WaitFor,
	) -> Result<ApiWaitForResult<Vec<RangeOrder>>> {
		// Submit the mint order
		Ok(into_api_wait_for_result(
			self.submit_signed_extrinsic_wait_for(
				pallet_cf_pools::Call::set_range_order {
					base_asset,
					quote_asset,
					id,
					option_tick_range,
					size,
				},
				wait_for,
			)
			.await?,
			collect_range_order_returns,
		))
	}

	async fn update_limit_order(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		side: Side,
		id: OrderId,
		option_tick: Option<Tick>,
		amount_change: IncreaseOrDecrease<AssetAmount>,
		dispatch_at: Option<BlockNumber>,
		wait_for: WaitFor,
	) -> Result<ApiWaitForResult<Vec<LimitOrder>>> {
		Ok(into_api_wait_for_result(
			self.submit_signed_extrinsic_wait_for(
				pallet_cf_pools::Call::update_limit_order {
					base_asset,
					quote_asset,
					side,
					id,
					option_tick,
					amount_change,
					dispatch_at,
				},
				wait_for,
			)
			.await?,
			collect_limit_order_returns,
		))
	}

	async fn set_limit_order(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		side: Side,
		id: OrderId,
		option_tick: Option<Tick>,
		sell_amount: AssetAmount,
		dispatch_at: Option<BlockNumber>,
		wait_for: WaitFor,
		close_order_at: Option<BlockNumber>,
	) -> Result<ApiWaitForResult<Vec<LimitOrder>>> {
		Ok(into_api_wait_for_result(
			self.submit_signed_extrinsic_wait_for(
				pallet_cf_pools::Call::set_limit_order {
					base_asset,
					quote_asset,
					side,
					id,
					option_tick,
					sell_amount,
					close_order_at,
					dispatch_at,
				},
				wait_for,
			)
			.await?,
			collect_limit_order_returns,
		))
	}

	async fn register_account(&self) -> Result<H256> {
		self.simple_submission_with_dry_run(pallet_cf_lp::Call::register_lp_account {})
			.await
	}

	async fn deregister_account(&self) -> Result<H256> {
		self.simple_submission_with_dry_run(pallet_cf_lp::Call::deregister_lp_account {})
			.await
	}

	async fn cancel_orders_batch(
		&self,
		orders: BoundedVec<CloseOrder, ConstU32<MAX_ORDERS_DELETE>>,
		wait_for: WaitFor,
	) -> Result<ApiWaitForResult<Vec<LimitOrRangeOrder>>> {
		Ok(into_api_wait_for_result(
			self.submit_signed_extrinsic_wait_for(
				pallet_cf_pools::Call::cancel_orders_batch { orders },
				wait_for,
			)
			.await?,
			collect_order_returns,
		))
	}

	async fn schedule_swap(
		&self,
		amount: AssetAmount,
		input_asset: Asset,
		output_asset: Asset,
		retry_duration: BlockNumber,
		price_limits: PriceLimits,
		dca_params: Option<DcaParameters>,
		wait_for: WaitFor,
	) -> Result<ApiWaitForResult<SwapRequestId>> {
		let wait_for_result = self
			.submit_signed_extrinsic_wait_for(
				pallet_cf_lp::Call::schedule_swap {
					amount,
					input_asset,
					output_asset,
					retry_duration,
					price_limits,
					dca_params,
				},
				wait_for,
			)
			.await?;

		Ok(match wait_for_result {
			WaitForResult::TransactionHash(tx_hash) => return Ok(ApiWaitForResult::TxHash(tx_hash)),
			WaitForResult::Details(extrinsic_data) => {
				let swap_request_id = extrinsic_data
					.events
					.into_iter()
					.find_map(|event| match event {
						state_chain_runtime::RuntimeEvent::Swapping(
							pallet_cf_swapping::Event::SwapRequested { swap_request_id, .. },
						) => Some(swap_request_id),
						_ => None,
					})
					.ok_or_else(|| anyhow!("No SwapRequested event was found"))?;

				ApiWaitForResult::TxDetails {
					tx_hash: extrinsic_data.tx_hash,
					response: swap_request_id,
				}
			},
		})
	}
}
