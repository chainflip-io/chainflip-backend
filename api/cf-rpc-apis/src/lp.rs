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

use crate::RpcResult;

use cf_chains::{address::AddressString, evm::U256};
use cf_primitives::{
	chains::assets::any::AssetMap, ApiWaitForResult, Asset, BasisPoints, BlockNumber,
	DcaParameters, EgressId, ForeignChain, Price, WaitFor,
};
use cf_rpc_types::{
	AccountId32, BlockUpdate, BoundedVec, ConstU32, EthereumAddress, Hash, NumberOrHex, OrderFills,
};
use jsonrpsee::proc_macros::rpc;
use std::ops::Range;

pub use cf_rpc_types::lp::*;

#[rpc(server, client, namespace = "lp")]
pub trait LpRpcApi {
	#[method(name = "register_account")]
	async fn register_account(&self) -> RpcResult<Hash>;

	#[deprecated(note = "Use `request_liquidity_deposit_address` instead")]
	#[method(name = "liquidity_deposit")]
	async fn request_liquidity_deposit_address_legacy(
		&self,
		asset: Asset,
		wait_for: Option<WaitFor>,
		boost_fee: Option<BasisPoints>,
	) -> RpcResult<ApiWaitForResult<AddressString>>;

	#[method(name = "request_liquidity_deposit_address")]
	async fn request_liquidity_deposit_address(
		&self,
		asset: Asset,
		wait_for: Option<WaitFor>,
		boost_fee: Option<BasisPoints>,
	) -> RpcResult<ApiWaitForResult<LiquidityDepositChannelDetails>>;

	#[method(name = "register_liquidity_refund_address")]
	async fn register_liquidity_refund_address(
		&self,
		chain: ForeignChain,
		address: AddressString,
	) -> RpcResult<Hash>;

	#[method(name = "withdraw_asset")]
	async fn withdraw_asset(
		&self,
		amount: NumberOrHex,
		asset: Asset,
		destination_address: AddressString,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<EgressId>>;

	#[method(name = "transfer_asset")]
	async fn transfer_asset(
		&self,
		amount: U256,
		asset: Asset,
		destination_account: AccountId32,
	) -> RpcResult<Hash>;

	#[method(name = "update_range_order")]
	async fn update_range_order(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		id: OrderIdJson,
		tick_range: Option<Range<Tick>>,
		size_change: IncreaseOrDecrease<RangeOrderSizeJson>,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<Vec<RangeOrder>>>;

	#[method(name = "set_range_order")]
	async fn set_range_order(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		id: OrderIdJson,
		tick_range: Option<Range<Tick>>,
		size: RangeOrderSizeJson,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<Vec<RangeOrder>>>;

	#[method(name = "update_limit_order")]
	async fn update_limit_order(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		side: Side,
		id: OrderIdJson,
		tick: Option<Tick>,
		amount_change: IncreaseOrDecrease<NumberOrHex>,
		dispatch_at: Option<BlockNumber>,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<Vec<LimitOrder>>>;

	#[method(name = "set_limit_order")]
	async fn set_limit_order(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		side: Side,
		id: OrderIdJson,
		tick: Option<Tick>,
		sell_amount: NumberOrHex,
		dispatch_at: Option<BlockNumber>,
		wait_for: Option<WaitFor>,
		close_order_at: Option<BlockNumber>,
	) -> RpcResult<ApiWaitForResult<Vec<LimitOrder>>>;

	#[method(name = "free_balances", aliases = ["lp_asset_balances"])]
	async fn free_balances(&self) -> RpcResult<AssetMap<U256>>;

	#[method(name = "get_open_swap_channels")]
	async fn get_open_swap_channels(&self, at: Option<Hash>) -> RpcResult<OpenSwapChannels>;

	#[method(name = "request_redemption")]
	async fn request_redemption(
		&self,
		redeem_address: EthereumAddress,
		exact_amount: Option<NumberOrHex>,
		executor_address: Option<EthereumAddress>,
	) -> RpcResult<Hash>;

	#[subscription(name = "subscribe_order_fills", item = BlockUpdate<OrderFills>)]
	async fn subscribe_order_fills(&self);

	#[method(name = "order_fills")]
	async fn order_fills(&self, at: Option<Hash>) -> RpcResult<BlockUpdate<OrderFills>>;

	#[method(name = "cancel_all_orders")]
	async fn cancel_all_orders(
		&self,
		wait_for: Option<WaitFor>,
	) -> RpcResult<Vec<ApiWaitForResult<Vec<LimitOrRangeOrder>>>>;

	#[method(name = "cancel_orders_batch")]
	async fn cancel_orders_batch(
		&self,
		orders: BoundedVec<CloseOrderJson, ConstU32<MAX_ORDERS_DELETE>>,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<Vec<LimitOrRangeOrder>>>;

	#[method(name = "schedule_swap")]
	async fn schedule_swap(
		&self,
		amount: NumberOrHex,
		input_asset: Asset,
		output_asset: Asset,
		retry_duration: BlockNumber,
		min_price: Price,
		dca_params: Option<DcaParameters>,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<SwapRequestResponse>>;
}
