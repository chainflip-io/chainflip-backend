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

use crate::{
	backend::{CustomRpcBackend, NotificationBehaviour},
	get_preallocated_channels, order_fills,
	pool_client::{is_transaction_status_error, PoolClientError, SignedPoolClient},
	CfApiError, RpcResult, StorageQueryApi,
};
use anyhow::anyhow;
use cf_amm::{common::Side, math::Tick};
use cf_chains::{
	address::{AddressString, ToHumanreadableAddress},
	eth::Address as EthereumAddress,
	instances::ChainInstanceFor,
	Chain,
};
use cf_node_client::{
	events_decoder::{DynamicEventError, DynamicEvents},
	extract_from_first_matching_event,
	subxt_state_chain_config::cf_static_runtime,
	ExtrinsicData, WaitForDynamicResult,
};
use cf_primitives::{
	chains::{assets::any::AssetMap, Arbitrum, Bitcoin, Ethereum, Polkadot, Solana},
	ApiWaitForResult, Asset, BasisPoints, BlockNumber, ChannelId, DcaParameters, EgressId,
	ForeignChain, PriceLimits, WaitFor,
};
use cf_rpc_apis::{
	lp::{
		CloseOrderJson, LimitOrRangeOrder, LimitOrder, LiquidityDepositChannelDetails,
		LpRpcApiServer, OpenSwapChannels, OrderIdJson, RangeOrder, RangeOrderChange,
		RangeOrderSizeJson, SwapRequestResponse,
	},
	ExtrinsicResponse, OrderFills, RedemptionAmount, SwapChannelInfo,
};
use cf_utilities::{rpc::NumberOrHex, try_parse_number_or_hex};
use frame_support::BoundedVec;
use futures::StreamExt;
use jsonrpsee::{core::async_trait, tokio, PendingSubscriptionSink};
use pallet_cf_ingress_egress::DepositChannelDetails;
use pallet_cf_pools::{CloseOrder, IncreaseOrDecrease, MAX_ORDERS_DELETE};
use sc_client_api::{
	blockchain::HeaderMetadata, Backend, BlockBackend, BlockchainEvents, ExecutorProvider,
	HeaderBackend, StorageProvider,
};
use sc_transaction_pool::FullPool;
use sc_transaction_pool_api::{TransactionStatus, TxIndex};
use sp_api::CallApiAt;
use sp_core::{crypto::AccountId32, U256};
use sp_runtime::traits::Block as BlockT;
use state_chain_runtime::{
	chainflip::BlockUpdate, runtime_apis::CustomRuntimeApi, AccountId, ConstU32, Hash, Nonce,
	RuntimeCall,
};
use std::{ops::Range, sync::Arc};

pub mod lp_crypto {
	use sp_application_crypto::{app_crypto, sr25519, KeyTypeId};
	/// Liquidity Provider Key Type ID used to store the key on state chain node keystore
	pub const LP_KEY_TYPE_ID: KeyTypeId = KeyTypeId(*b"lqpr");

	app_crypto!(sr25519, LP_KEY_TYPE_ID);
}

/// An LP signed RPC extension for the state chain node.
pub struct LpSignedRpc<C, B, BE>
where
	B: BlockT<Hash = state_chain_runtime::Hash, Header = state_chain_runtime::Header>,
	BE: Send + Sync + 'static + Backend<B>,
	C: Send
		+ Sync
		+ 'static
		+ BlockBackend<B>
		+ HeaderBackend<B>
		+ HeaderMetadata<B, Error = sc_client_api::blockchain::Error>
		+ CallApiAt<B>
		+ StorageProvider<B, BE>
		+ sp_api::ProvideRuntimeApi<B>
		+ sp_runtime::traits::BlockIdTo<B>,
	C::Api: CustomRuntimeApi<B>
		+ sp_api::Core<B>
		+ sp_api::Metadata<B>
		+ sp_block_builder::BlockBuilder<B>
		+ sp_transaction_pool::runtime_api::TaggedTransactionQueue<B>
		+ frame_system_rpc_runtime_api::AccountNonceApi<B, AccountId, Nonce>,
{
	pub rpc_backend: CustomRpcBackend<C, B, BE>,
	pub signed_pool_client: SignedPoolClient<C, B, BE>,
}

impl<C, B, BE> LpSignedRpc<C, B, BE>
where
	B: BlockT<Hash = state_chain_runtime::Hash, Header = state_chain_runtime::Header>,
	BE: Send + Sync + 'static + Backend<B>,
	C: Send
		+ Sync
		+ 'static
		+ BlockBackend<B>
		+ HeaderBackend<B>
		+ HeaderMetadata<B, Error = sc_client_api::blockchain::Error>
		+ CallApiAt<B>
		+ StorageProvider<B, BE>
		+ sp_api::ProvideRuntimeApi<B>
		+ sp_runtime::traits::BlockIdTo<B>,
	C::Api: CustomRuntimeApi<B>
		+ sp_api::Core<B>
		+ sp_api::Metadata<B>
		+ sp_block_builder::BlockBuilder<B>
		+ sp_transaction_pool::runtime_api::TaggedTransactionQueue<B>
		+ frame_system_rpc_runtime_api::AccountNonceApi<B, AccountId, Nonce>,
{
	pub fn new(
		client: Arc<C>,
		backend: Arc<BE>,
		executor: Arc<dyn sp_core::traits::SpawnNamed>,
		pool: Arc<FullPool<B, C>>,
		pair: sp_core::sr25519::Pair,
	) -> Self {
		Self {
			rpc_backend: CustomRpcBackend::new(client.clone(), backend, executor),
			signed_pool_client: SignedPoolClient::new(client, pool, pair),
		}
	}

	async fn extract_liquidity_deposit_channel_details(
		&self,
		block_hash: Hash,
		tx_index: TxIndex,
	) -> RpcResult<(ChannelId, LiquidityDepositChannelDetails)> {
		let ExtrinsicData { events, .. } = self
			.signed_pool_client
			.get_extrinsic_data_dynamic(block_hash, tx_index)
			.await
			.map_err(CfApiError::from)?;

		Ok(extract_from_first_matching_event!(
			events,
			cf_static_runtime::liquidity_provider::events::LiquidityDepositAddressReady,
			{ channel_id, deposit_address, deposit_chain_expiry_block },
			(channel_id, LiquidityDepositChannelDetails {
					deposit_address: AddressString::from_encoded_address(deposit_address.0),
					deposit_chain_expiry_block,
			})
		)
		.map_err(CfApiError::from)?)
	}
}

#[async_trait]
impl<C, B, BE> LpRpcApiServer for LpSignedRpc<C, B, BE>
where
	B: BlockT<Hash = state_chain_runtime::Hash, Header = state_chain_runtime::Header>,
	BE: Send + Sync + 'static + Backend<B>,
	C: Send
		+ Sync
		+ 'static
		+ BlockBackend<B>
		+ HeaderBackend<B>
		+ HeaderMetadata<B, Error = sc_client_api::blockchain::Error>
		+ BlockchainEvents<B>
		+ ExecutorProvider<B>
		+ CallApiAt<B>
		+ StorageProvider<B, BE>
		+ sp_api::ProvideRuntimeApi<B>
		+ sp_runtime::traits::BlockIdTo<B>,
	C::Api: CustomRuntimeApi<B>
		+ sp_api::Core<B>
		+ sp_api::Metadata<B>
		+ sp_block_builder::BlockBuilder<B>
		+ sp_transaction_pool::runtime_api::TaggedTransactionQueue<B>
		+ frame_system_rpc_runtime_api::AccountNonceApi<B, AccountId, Nonce>,
{
	async fn register_account(&self) -> RpcResult<state_chain_runtime::Hash> {
		let ExtrinsicData { tx_hash, .. } = self
			.signed_pool_client
			.submit_watch_dynamic(
				RuntimeCall::from(pallet_cf_lp::Call::register_lp_account {}),
				false,
				true,
			)
			.await
			.map_err(CfApiError::from)?;

		Ok(tx_hash)
	}

	async fn request_liquidity_deposit_address(
		&self,
		asset: Asset,
		wait_for: Option<WaitFor>,
		boost_fee: Option<BasisPoints>,
	) -> RpcResult<ApiWaitForResult<LiquidityDepositChannelDetails>> {
		let wait_for_param = match wait_for {
			Some(WaitFor::InBlock) => Err(anyhow!(
				"InBlock waiting is not allowed for this method. \
				Use request_liquidity_deposit_address_v2 instead."
			))?,
			Some(value) => value,
			None => WaitFor::Finalized,
		};

		Ok(
			match self
				.signed_pool_client
				.submit_wait_for_result_dynamic(
					RuntimeCall::from(pallet_cf_lp::Call::request_liquidity_deposit_address {
						asset,
						boost_fee: boost_fee.unwrap_or_default(),
					}),
					wait_for_param,
					false,
				)
				.await
				.map_err(CfApiError::from)?
			{
				WaitForDynamicResult::TransactionHash(tx_hash) => ApiWaitForResult::TxHash(tx_hash),
				WaitForDynamicResult::Data(extrinsic_data) => extract_from_first_matching_event!(
					extrinsic_data.events,
					cf_static_runtime::liquidity_provider::events::LiquidityDepositAddressReady,
					{ deposit_address, deposit_chain_expiry_block },
					ApiWaitForResult::TxDetails {
						tx_hash: extrinsic_data.tx_hash,
						response: LiquidityDepositChannelDetails {
							deposit_address: AddressString::from_encoded_address(deposit_address.0),
							deposit_chain_expiry_block,
						}
					}
				)
				.map_err(CfApiError::from)?,
			},
		)
	}

	async fn request_liquidity_deposit_address_v2(
		&self,
		asset: Asset,
		boost_fee: Option<BasisPoints>,
	) -> RpcResult<ExtrinsicResponse<LiquidityDepositChannelDetails>> {
		let mut status_stream = self
			.signed_pool_client
			.submit_watch(
				RuntimeCall::from(pallet_cf_lp::Call::request_liquidity_deposit_address {
					asset,
					boost_fee: boost_fee.unwrap_or_default(),
				}),
				false,
			)
			.await
			.map_err(CfApiError::from)?;

		// Get the pre-allocated channels from the previous finalized block
		let pre_allocated_channels = get_preallocated_channels(
			&self.rpc_backend,
			self.signed_pool_client.account_id(),
			asset.into(),
		)?;

		while let Some(status) = status_stream.next().await {
			match status {
				TransactionStatus::InBlock((block_hash, tx_index)) => {
					let (channel_id, channel_details) = self
						.extract_liquidity_deposit_channel_details(block_hash, tx_index)
						.await?;

					// If the extracted deposit channel was pre-allocated to this lp
					// in the previous finalized block, we can return it immediately.
					// Otherwise, we need to wait for the transaction to be finalized.
					if pre_allocated_channels.contains(&channel_id) {
						return Ok(ExtrinsicResponse {
							block_number: self.rpc_backend.block_number_for(block_hash)?,
							block_hash,
							tx_index,
							response: channel_details,
						});
					}
				},
				TransactionStatus::Finalized((block_hash, tx_index)) => {
					let (_, channel_details) = self
						.extract_liquidity_deposit_channel_details(block_hash, tx_index)
						.await?;
					return Ok(ExtrinsicResponse {
						block_number: self.rpc_backend.block_number_for(block_hash)?,
						block_hash,
						tx_index,
						response: channel_details,
					});
				},
				_ => is_transaction_status_error(&status).map_err(CfApiError::from)?,
			}
		}
		Err(CfApiError::from(PoolClientError::UnexpectedEndOfStream))?
	}

	async fn register_liquidity_refund_address(
		&self,
		chain: ForeignChain,
		address: AddressString,
	) -> RpcResult<Hash> {
		let ExtrinsicData { tx_hash, .. } = self
			.signed_pool_client
			.submit_watch_dynamic(
				RuntimeCall::from(pallet_cf_lp::Call::register_liquidity_refund_address {
					address: address
						.try_parse_to_encoded_address(chain)
						.map_err(anyhow::Error::msg)?,
				}),
				false,
				false,
			)
			.await
			.map_err(CfApiError::from)?;

		Ok(tx_hash)
	}

	async fn withdraw_asset(
		&self,
		amount: NumberOrHex,
		asset: Asset,
		destination_address: AddressString,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<EgressId>> {
		let amount = try_parse_number_or_hex(amount)?;
		if amount == 0 {
			Err(anyhow!("Withdrawal amount must be greater than 0"))?;
		}

		Ok(
			match self
				.signed_pool_client
				.submit_wait_for_result_dynamic(
					RuntimeCall::from(pallet_cf_lp::Call::withdraw_asset {
						amount,
						asset,
						destination_address: destination_address
							.try_parse_to_encoded_address(asset.into())
							.map_err(anyhow::Error::msg)?,
					}),
					wait_for.unwrap_or_default(),
					false,
				)
				.await
				.map_err(CfApiError::from)?
			{
				WaitForDynamicResult::TransactionHash(tx_hash) => ApiWaitForResult::TxHash(tx_hash),
				WaitForDynamicResult::Data(extrinsic_data) => extract_from_first_matching_event!(
					extrinsic_data.events,
					cf_static_runtime::liquidity_provider::events::WithdrawalEgressScheduled,
					{ egress_id },
					ApiWaitForResult::TxDetails {
						tx_hash: extrinsic_data.tx_hash,
						response: (egress_id.0 .0, egress_id.1)
					}
				)
				.map_err(CfApiError::from)?,
			},
		)
	}

	async fn transfer_asset(
		&self,
		amount: U256,
		asset: Asset,
		destination_account: AccountId32,
	) -> RpcResult<Hash> {
		let amount = amount.try_into().map_err(|_| anyhow!("Failed to convert amount to u128"))?;
		if amount == 0 {
			Err(anyhow!("Amount must be greater than 0"))?;
		}

		let ExtrinsicData { tx_hash, .. } = self
			.signed_pool_client
			.submit_watch_dynamic(
				RuntimeCall::from(pallet_cf_lp::Call::transfer_asset {
					amount,
					asset,
					destination: destination_account,
				}),
				false,
				false,
			)
			.await
			.map_err(CfApiError::from)?;

		Ok(tx_hash)
	}

	async fn update_range_order(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		id: OrderIdJson,
		tick_range: Option<Range<Tick>>,
		size_change: IncreaseOrDecrease<RangeOrderSizeJson>,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<Vec<RangeOrder>>> {
		Ok(into_api_wait_for_dynamic_result(
			self.signed_pool_client
				.submit_wait_for_result_dynamic(
					RuntimeCall::from(pallet_cf_pools::Call::update_range_order {
						base_asset,
						quote_asset,
						id: id.try_into()?,
						option_tick_range: tick_range,
						size_change: size_change.try_map(|size| size.try_into())?,
					}),
					wait_for.unwrap_or_default(),
					false,
				)
				.await
				.map_err(CfApiError::from)?,
			filter_range_orders,
		)
		.map_err(CfApiError::from)?)
	}

	async fn set_range_order(
		&self,
		base_asset: Asset,
		quote_asset: Asset,
		id: OrderIdJson,
		tick_range: Option<Range<Tick>>,
		size: RangeOrderSizeJson,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<Vec<RangeOrder>>> {
		Ok(into_api_wait_for_dynamic_result(
			self.signed_pool_client
				.submit_wait_for_result_dynamic(
					RuntimeCall::from(pallet_cf_pools::Call::set_range_order {
						base_asset,
						quote_asset,
						id: id.try_into()?,
						option_tick_range: tick_range,
						size: size.try_into()?,
					}),
					wait_for.unwrap_or_default(),
					false,
				)
				.await
				.map_err(CfApiError::from)?,
			filter_range_orders,
		)
		.map_err(CfApiError::from)?)
	}

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
	) -> RpcResult<ApiWaitForResult<Vec<LimitOrder>>> {
		Ok(into_api_wait_for_dynamic_result(
			self.signed_pool_client
				.submit_wait_for_result_dynamic(
					RuntimeCall::from(pallet_cf_pools::Call::update_limit_order {
						base_asset,
						quote_asset,
						side,
						id: id.try_into()?,
						option_tick: tick,
						amount_change: amount_change.try_map(try_parse_number_or_hex)?,
						dispatch_at,
					}),
					wait_for.unwrap_or_default(),
					false,
				)
				.await
				.map_err(CfApiError::from)?,
			filter_limit_orders,
		)
		.map_err(CfApiError::from)?)
	}

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
	) -> RpcResult<ApiWaitForResult<Vec<LimitOrder>>> {
		Ok(into_api_wait_for_dynamic_result(
			self.signed_pool_client
				.submit_wait_for_result_dynamic(
					RuntimeCall::from(pallet_cf_pools::Call::set_limit_order {
						base_asset,
						quote_asset,
						side,
						id: id.try_into()?,
						option_tick: tick,
						sell_amount: try_parse_number_or_hex(sell_amount)?,
						close_order_at,
						dispatch_at,
					}),
					wait_for.unwrap_or_default(),
					false,
				)
				.await
				.map_err(CfApiError::from)?,
			filter_limit_orders,
		)
		.map_err(CfApiError::from)?)
	}

	async fn free_balances(&self) -> RpcResult<AssetMap<U256>> {
		Ok(self
			.rpc_backend
			.client
			.runtime_api()
			.cf_free_balances(
				self.rpc_backend.client.info().finalized_hash,
				self.signed_pool_client.account_id(),
			)
			.map_err(CfApiError::from)?
			.map(Into::into))
	}

	async fn get_open_swap_channels(&self, at: Option<Hash>) -> RpcResult<OpenSwapChannels> {
		let (ethereum, bitcoin, polkadot, arbitrum, solana) = tokio::try_join!(
			self.get_open_swap_channels_for_chain::<Ethereum>(at),
			self.get_open_swap_channels_for_chain::<Bitcoin>(at),
			self.get_open_swap_channels_for_chain::<Polkadot>(at),
			self.get_open_swap_channels_for_chain::<Arbitrum>(at),
			self.get_open_swap_channels_for_chain::<Solana>(at),
		)?;
		Ok(OpenSwapChannels { ethereum, bitcoin, polkadot, arbitrum, solana })
	}

	async fn request_redemption(
		&self,
		redeem_address: EthereumAddress,
		exact_amount: Option<NumberOrHex>,
		executor_address: Option<EthereumAddress>,
	) -> RpcResult<Hash> {
		let redeem_amount = if let Some(number_or_hex) = exact_amount {
			RedemptionAmount::Exact(try_parse_number_or_hex(number_or_hex)?)
		} else {
			RedemptionAmount::Max
		};

		let ExtrinsicData { tx_hash, .. } = self
			.signed_pool_client
			.submit_watch_dynamic(
				RuntimeCall::from(pallet_cf_funding::Call::redeem {
					amount: redeem_amount,
					address: redeem_address,
					executor: executor_address,
				}),
				false,
				true,
			)
			.await
			.map_err(CfApiError::from)?;

		Ok(tx_hash)
	}

	async fn subscribe_order_fills(&self, sink: PendingSubscriptionSink, wait_finalized: Option<bool>) {
		self.rpc_backend
			.new_subscription(
				if wait_finalized.unwrap_or(true) { NotificationBehaviour::Finalized } else { NotificationBehaviour::Best },
				false,
				true,
				sink,
				move |client, hash| order_fills::order_fills_for_block(client, hash),
			)
			.await
	}

	// TODO: is also defined in lib.rs consider defining in common place
	async fn order_fills(&self, at: Option<Hash>) -> RpcResult<BlockUpdate<OrderFills>> {
		order_fills::order_fills_for_block(
			self.rpc_backend.client.as_ref(),
			at.unwrap_or_else(|| self.rpc_backend.client.info().finalized_hash),
		)
	}

	async fn cancel_all_orders(
		&self,
		wait_for: Option<WaitFor>,
	) -> RpcResult<Vec<ApiWaitForResult<Vec<LimitOrRangeOrder>>>> {
		let mut orders_to_delete: Vec<CloseOrder> = vec![];

		let pool_pairs = self
			.rpc_backend
			.client
			.runtime_api()
			.cf_pools(self.rpc_backend.client.info().best_hash)
			.map_err(CfApiError::from)?;

		for pool in pool_pairs {
			let orders = match self.rpc_backend.client.runtime_api().cf_pool_orders(
				self.rpc_backend.client.info().best_hash,
				pool.base,
				pool.quote,
				Some(self.signed_pool_client.account_id()),
				false,
			) {
				Ok(Ok(r)) => Ok(r),
				Ok(Err(e)) => Err(CfApiError::DispatchError(e)),
				Err(e) => Err(CfApiError::RuntimeApiError(e)),
			}?;

			for order in orders.range_orders {
				orders_to_delete.push(CloseOrder::Range {
					base_asset: pool.base,
					quote_asset: pool.quote,
					id: order.id.try_into().expect("Internal AMM OrderId is a u64"),
				});
			}
			for order in orders.limit_orders.asks {
				orders_to_delete.push(CloseOrder::Limit {
					base_asset: pool.base,
					quote_asset: pool.quote,
					side: Side::Sell,
					id: order.id.try_into().expect("Internal AMM OrderId is a u64"),
				});
			}
			for order in orders.limit_orders.bids {
				orders_to_delete.push(CloseOrder::Limit {
					base_asset: pool.base,
					quote_asset: pool.quote,
					side: Side::Buy,
					id: order.id.try_into().expect("Internal AMM OrderId is a u64"),
				});
			}
		}

		// in case there are more than 100 elements we need to split the orders into chunks of 100
		// and submit multiple extrinsics
		let mut extrinsic_submissions = vec![];
		for order_chunk in orders_to_delete.chunks(MAX_ORDERS_DELETE as usize) {
			extrinsic_submissions.push(
				self.cancel_orders_batch_call(
					BoundedVec::<_, ConstU32<MAX_ORDERS_DELETE>>::try_from(order_chunk.to_vec())
						.expect("Guaranteed by `chunk` method."),
					wait_for,
				)
				.await?,
			);
		}

		Ok(extrinsic_submissions)
	}

	async fn cancel_orders_batch(
		&self,
		orders: BoundedVec<CloseOrderJson, ConstU32<MAX_ORDERS_DELETE>>,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<Vec<LimitOrRangeOrder>>> {
		self.cancel_orders_batch_call(
			orders
				.into_iter()
				.map(TryInto::try_into)
				.collect::<Result<Vec<_>, _>>()?
				.try_into()
				.expect("Impossible to fail, given the same MAX_ORDERS_DELETE"),
			wait_for,
		)
		.await
	}

	async fn schedule_swap(
		&self,
		amount: NumberOrHex,
		input_asset: Asset,
		output_asset: Asset,
		retry_duration: BlockNumber,
		price_limits: PriceLimits,
		dca_params: Option<DcaParameters>,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<SwapRequestResponse>> {
		Ok(
			match self
				.signed_pool_client
				.submit_wait_for_result_dynamic(
					RuntimeCall::from(pallet_cf_lp::Call::schedule_swap {
						amount: try_parse_number_or_hex(amount)?,
						input_asset,
						output_asset,
						retry_duration,
						price_limits,
						dca_params,
					}),
					wait_for.unwrap_or_default(),
					false,
				)
				.await
				.map_err(CfApiError::from)?
			{
				WaitForDynamicResult::TransactionHash(tx_hash) => ApiWaitForResult::TxHash(tx_hash),
				WaitForDynamicResult::Data(extrinsic_data) => extract_from_first_matching_event!(
					extrinsic_data.events,
					cf_static_runtime::swapping::events::SwapRequested,
					{ swap_request_id },
					ApiWaitForResult::TxDetails {
						tx_hash: extrinsic_data.tx_hash,
						response: swap_request_id.0.into()
					}
				)
				.map_err(CfApiError::from)?,
			},
		)
	}
}

impl<C, B, BE> LpSignedRpc<C, B, BE>
where
	B: BlockT<Hash = state_chain_runtime::Hash, Header = state_chain_runtime::Header>,
	BE: Send + Sync + 'static + Backend<B>,
	C: Send
		+ Sync
		+ 'static
		+ BlockBackend<B>
		+ HeaderBackend<B>
		+ HeaderMetadata<B, Error = sc_client_api::blockchain::Error>
		+ CallApiAt<B>
		+ StorageProvider<B, BE>
		+ sp_api::ProvideRuntimeApi<B>
		+ sp_runtime::traits::BlockIdTo<B>,
	C::Api: CustomRuntimeApi<B>
		+ sp_api::Core<B>
		+ sp_api::Metadata<B>
		+ sp_block_builder::BlockBuilder<B>
		+ sp_transaction_pool::runtime_api::TaggedTransactionQueue<B>
		+ frame_system_rpc_runtime_api::AccountNonceApi<B, AccountId, Nonce>,
{
	async fn cancel_orders_batch_call(
		&self,
		orders: BoundedVec<CloseOrder, ConstU32<MAX_ORDERS_DELETE>>,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<Vec<LimitOrRangeOrder>>> {
		Ok(into_api_wait_for_dynamic_result(
			self.signed_pool_client
				.submit_wait_for_result_dynamic(
					RuntimeCall::from(pallet_cf_pools::Call::cancel_orders_batch { orders }),
					wait_for.unwrap_or_default(),
					false,
				)
				.await
				.map_err(CfApiError::from)?,
			filter_orders,
		)
		.map_err(CfApiError::from)?)
	}

	pub async fn get_open_swap_channels_for_chain<CH: Chain>(
		&self,
		block_hash: Option<Hash>,
	) -> RpcResult<Vec<SwapChannelInfo<CH>>>
	where
		state_chain_runtime::Runtime:
			pallet_cf_ingress_egress::Config<ChainInstanceFor<CH>, TargetChain = CH>,
	{
		let block_hash =
			block_hash.unwrap_or_else(|| self.rpc_backend.client.info().finalized_hash);

		let channels = StorageQueryApi::new(&self.rpc_backend.client)
			.collect_from_storage_map::<pallet_cf_ingress_egress::DepositChannelLookup<
			state_chain_runtime::Runtime,
			ChainInstanceFor<CH>,
		>, _, _, Vec<_>>(block_hash)?;

		let network_environment = StorageQueryApi::new(&self.rpc_backend.client)
			.get_storage_value::<pallet_cf_environment::ChainflipNetworkEnvironment<
			state_chain_runtime::Runtime,
		>, _>(block_hash)?;

		Ok(channels
			.into_iter()
			.filter_map(|(_, DepositChannelDetails { action, deposit_channel, .. })| match action {
				pallet_cf_ingress_egress::ChannelAction::Swap { destination_asset, .. } =>
					Some(SwapChannelInfo {
						deposit_address: deposit_channel
							.address
							.to_humanreadable(network_environment),
						source_asset: deposit_channel.asset.into(),
						destination_asset,
					}),
				_ => None,
			})
			.collect::<Vec<_>>())
	}
}

fn into_api_wait_for_dynamic_result<T>(
	from: WaitForDynamicResult,
	map_events: impl FnOnce(&DynamicEvents) -> Result<T, DynamicEventError>,
) -> Result<ApiWaitForResult<T>, DynamicEventError> {
	match from {
		WaitForDynamicResult::TransactionHash(tx_hash) => Ok(ApiWaitForResult::TxHash(tx_hash)),
		WaitForDynamicResult::Data(extrinsic_data) => Ok(ApiWaitForResult::TxDetails {
			tx_hash: extrinsic_data.tx_hash,
			response: map_events(&extrinsic_data.events)?,
		}),
	}
}

pub fn filter_orders(events: &DynamicEvents) -> Result<Vec<LimitOrRangeOrder>, DynamicEventError> {
	Ok(filter_limit_orders(events)?
		.into_iter()
		.map(LimitOrRangeOrder::LimitOrder)
		.chain(filter_range_orders(events)?.into_iter().map(LimitOrRangeOrder::RangeOrder))
		.collect())
}

pub fn filter_range_orders(events: &DynamicEvents) -> Result<Vec<RangeOrder>, DynamicEventError> {
	Ok(events
		.find_all_static_events::<cf_static_runtime::liquidity_pools::events::RangeOrderUpdated>(
			false,
		)?
		.into_iter()
		.map(|event| -> RangeOrder {
			// Convert from cf_static_runtime generated types
			let size_change = event
				.size_change
				.map(Into::<IncreaseOrDecrease<pallet_cf_pools::RangeOrderChange>>::into);
			let collected_fees =
				Into::<pallet_cf_pools::pallet::AssetAmounts>::into(event.collected_fees);

			RangeOrder {
				base_asset: event.base_asset.0,
				quote_asset: event.quote_asset.0,
				id: event.id.into(),
				size_change: size_change.map(|increase_or_decrease| {
					increase_or_decrease.map(|range_order_change| RangeOrderChange {
						liquidity: range_order_change.liquidity.into(),
						amounts: range_order_change.amounts.map(|amount| amount.into()),
					})
				}),
				liquidity_total: event.liquidity_total.into(),
				tick_range: Range { start: event.tick_range.start, end: event.tick_range.end },
				collected_fees: collected_fees.map(Into::into),
			}
		})
		.collect::<Vec<_>>())
}

pub fn filter_limit_orders(events: &DynamicEvents) -> Result<Vec<LimitOrder>, DynamicEventError> {
	Ok(events
		.find_all_static_events::<cf_static_runtime::liquidity_pools::events::LimitOrderUpdated>(
			false,
		)?
		.into_iter()
		.map(|event| -> LimitOrder {
			// Convert from cf_static_runtime generated types
			let sell_amount_change =
				event.sell_amount_change.map(Into::<IncreaseOrDecrease<_>>::into);

			LimitOrder {
				base_asset: event.base_asset.0,
				quote_asset: event.quote_asset.0,
				side: event.side.0,
				id: event.id.into(),
				tick: event.tick,
				sell_amount_total: event.sell_amount_total.into(),
				collected_fees: event.collected_fees.into(),
				bought_amount: event.bought_amount.into(),
				sell_amount_change: sell_amount_change
					.map(|increase_or_decrease| increase_or_decrease.map(|amount| amount.into())),
			}
		})
		.collect::<Vec<_>>())
}
