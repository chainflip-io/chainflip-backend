use crate::{pool_client::SignedPoolClient, CfApiError, RpcResult, StorageQueryApi};
use anyhow::anyhow;
use cf_amm::{common::Side, math::Tick};
use cf_chains::{
	address::{AddressString, ToHumanreadableAddress},
	arb::U256,
	eth::Address as EthereumAddress,
	instances::ChainInstanceFor,
	Chain,
};
use cf_node_clients::{WaitFor, WaitForResult};
use cf_primitives::{
	chains::{assets::any::AssetMap, Bitcoin, Ethereum, Polkadot},
	Asset, BasisPoints, BlockNumber, EgressId, ForeignChain,
};
use cf_rpc_types::{
	extract_event,
	lp::{
		collect_limit_order_returns, collect_order_returns, collect_range_order_returns,
		ApiWaitForResult, LimitOrRangeOrder, LimitOrder, OpenSwapChannels, OrderIdJson, RangeOrder,
		RangeOrderSizeJson,
	},
	RedemptionAmount, SwapChannelInfo,
};
use cf_utilities::{rpc::NumberOrHex, try_parse_number_or_hex};
use frame_support::BoundedVec;
use jsonrpsee::{core::async_trait, proc_macros::rpc, tokio};
use pallet_cf_ingress_egress::DepositChannelDetails;
use pallet_cf_pools::{CloseOrder, IncreaseOrDecrease, MAX_ORDERS_DELETE};
use sc_client_api::{
	blockchain::HeaderMetadata, Backend, BlockBackend, HeaderBackend, StorageProvider,
};
use sp_api::{CallApiAt, Core};
use sp_core::crypto::AccountId32;
use sp_runtime::traits::Block as BlockT;
use state_chain_runtime::{
	runtime_apis::CustomRuntimeApi, AccountId, ConstU32, Hash, Nonce, RuntimeCall,
};
use std::{ops::Range, sync::Arc};

#[rpc(server, client, namespace = "lp")]
pub trait LpSignedApi {
	#[method(name = "register_account")]
	async fn register_account(&self) -> RpcResult<state_chain_runtime::Hash>;

	#[method(name = "liquidity_deposit")]
	async fn request_liquidity_deposit_address(
		&self,
		asset: Asset,
		wait_for: Option<WaitFor>,
		boost_fee: Option<BasisPoints>,
	) -> RpcResult<ApiWaitForResult<String>>;

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
	) -> RpcResult<ApiWaitForResult<Vec<LimitOrder>>>;

	#[method(name = "free_balances", aliases = ["lp_asset_balances"])]
	async fn free_balances(&self) -> RpcResult<AssetMap<U256>>;

	#[method(name = "get_open_swap_channels")]
	async fn get_open_swap_channels(&self) -> RpcResult<OpenSwapChannels>;

	#[method(name = "request_redemption")]
	async fn request_redemption(
		&self,
		redeem_address: EthereumAddress,
		exact_amount: Option<NumberOrHex>,
		executor_address: Option<EthereumAddress>,
	) -> RpcResult<Hash>;

	// #[subscription(name = "subscribe_order_fills", item = BlockUpdate<OrderFills>)] is aliased
	// in custom_rpc because it just a pass-through

	// #[method(name = "order_fills")] is aliased in custom_rpc because it just a pass-through

	#[method(name = "cancel_all_orders")]
	async fn cancel_all_orders(
		&self,
		wait_for: Option<WaitFor>,
	) -> RpcResult<Vec<ApiWaitForResult<Vec<LimitOrRangeOrder>>>>;

	#[method(name = "cancel_orders_batch")]
	async fn cancel_orders_batch(
		&self,
		orders: BoundedVec<CloseOrder, ConstU32<MAX_ORDERS_DELETE>>,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<Vec<LimitOrRangeOrder>>>;
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
		+ Core<B>
		+ sp_block_builder::BlockBuilder<B>
		+ sp_transaction_pool::runtime_api::TaggedTransactionQueue<B>
		+ frame_system_rpc_runtime_api::AccountNonceApi<B, AccountId, Nonce>,
{
	pub client: Arc<C>,
	pub signed_pool_client: SignedPoolClient<C, B, BE>,
}

#[async_trait]
impl<C, B, BE> LpSignedApiServer for LpSignedRpc<C, B, BE>
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
		+ Core<B>
		+ sp_block_builder::BlockBuilder<B>
		+ sp_transaction_pool::runtime_api::TaggedTransactionQueue<B>
		+ frame_system_rpc_runtime_api::AccountNonceApi<B, AccountId, Nonce>,
{
	async fn register_account(&self) -> RpcResult<state_chain_runtime::Hash> {
		let (tx_hash, _, _, _) = self
			.signed_pool_client
			.submit_watch(
				RuntimeCall::from(pallet_cf_lp::Call::register_lp_account {}),
				false,
				true,
				None,
			)
			.await?;

		Ok(tx_hash)
	}

	async fn request_liquidity_deposit_address(
		&self,
		asset: Asset,
		wait_for: Option<WaitFor>,
		boost_fee: Option<BasisPoints>,
	) -> RpcResult<ApiWaitForResult<String>> {
		Ok(
			match self
				.signed_pool_client
				.submit_wait_for_result(
					RuntimeCall::from(pallet_cf_lp::Call::request_liquidity_deposit_address {
						asset,
						boost_fee: boost_fee.unwrap_or_default(),
					}),
					wait_for.unwrap_or_default(),
					false,
					None,
				)
				.await?
			{
				WaitForResult::TransactionHash(tx_hash) => ApiWaitForResult::TxHash(tx_hash),
				WaitForResult::Details(details) => {
					let (tx_hash, events, ..) = details;
					extract_event!(
						events,
						state_chain_runtime::RuntimeEvent::LiquidityProvider,
						pallet_cf_lp::Event::LiquidityDepositAddressReady,
						{ deposit_address, .. },
						ApiWaitForResult::TxDetails {
							tx_hash,
							response: deposit_address.to_string()
						}
					)?
				},
			},
		)
	}

	async fn register_liquidity_refund_address(
		&self,
		chain: ForeignChain,
		address: AddressString,
	) -> RpcResult<Hash> {
		let (tx_hash, _, _, _) = self
			.signed_pool_client
			.submit_watch(
				RuntimeCall::from(pallet_cf_lp::Call::register_liquidity_refund_address {
					address: address
						.try_parse_to_encoded_address(chain)
						.map_err(anyhow::Error::msg)?,
				}),
				false,
				false,
				None,
			)
			.await?;

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
				.submit_wait_for_result(
					RuntimeCall::from(pallet_cf_lp::Call::withdraw_asset {
						amount,
						asset,
						destination_address: destination_address
							.try_parse_to_encoded_address(asset.into())
							.map_err(anyhow::Error::msg)?,
					}),
					wait_for.unwrap_or_default(),
					false,
					None,
				)
				.await?
			{
				WaitForResult::TransactionHash(tx_hash) => ApiWaitForResult::TxHash(tx_hash),
				WaitForResult::Details(details) => {
					let (tx_hash, events, ..) = details;
					extract_event!(
						events,
						state_chain_runtime::RuntimeEvent::LiquidityProvider,
						pallet_cf_lp::Event::WithdrawalEgressScheduled,
						{ egress_id, .. },
						ApiWaitForResult::TxDetails {
							tx_hash,
							response: *egress_id
						}
					)?
				},
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

		let (tx_hash, _, _, _) = self
			.signed_pool_client
			.submit_watch(
				RuntimeCall::from(pallet_cf_lp::Call::transfer_asset {
					amount,
					asset,
					destination: destination_account,
				}),
				false,
				false,
				None,
			)
			.await?;

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
		Ok(into_api_wait_for_result(
			self.signed_pool_client
				.submit_wait_for_result(
					RuntimeCall::from(pallet_cf_pools::Call::update_range_order {
						base_asset,
						quote_asset,
						id: id.try_into()?,
						option_tick_range: tick_range,
						size_change: size_change.try_map(|size| size.try_into())?,
					}),
					wait_for.unwrap_or_default(),
					false,
					None,
				)
				.await?,
			collect_range_order_returns,
		))
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
		Ok(into_api_wait_for_result(
			self.signed_pool_client
				.submit_wait_for_result(
					RuntimeCall::from(pallet_cf_pools::Call::set_range_order {
						base_asset,
						quote_asset,
						id: id.try_into()?,
						option_tick_range: tick_range,
						size: size.try_into()?,
					}),
					wait_for.unwrap_or_default(),
					false,
					None,
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
		id: OrderIdJson,
		tick: Option<Tick>,
		amount_change: IncreaseOrDecrease<NumberOrHex>,
		dispatch_at: Option<BlockNumber>,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<Vec<LimitOrder>>> {
		self.scheduled_or_immediate(
			pallet_cf_pools::Call::update_limit_order {
				base_asset,
				quote_asset,
				side,
				id: id.try_into()?,
				option_tick: tick,
				amount_change: amount_change.try_map(try_parse_number_or_hex)?,
			},
			dispatch_at,
			wait_for.unwrap_or_default(),
		)
		.await
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
	) -> RpcResult<ApiWaitForResult<Vec<LimitOrder>>> {
		self.scheduled_or_immediate(
			pallet_cf_pools::Call::set_limit_order {
				base_asset,
				quote_asset,
				side,
				id: id.try_into()?,
				option_tick: tick,
				sell_amount: try_parse_number_or_hex(sell_amount)?,
			},
			dispatch_at,
			wait_for.unwrap_or_default(),
		)
		.await
	}

	async fn free_balances(&self) -> RpcResult<AssetMap<U256>> {
		self.client
			.runtime_api()
			.cf_free_balances(
				self.client.info().finalized_hash,
				self.signed_pool_client.account_id(),
			)
			.map_err(CfApiError::RuntimeApiError)
			.map(|asset_map| asset_map.map(Into::into))
	}

	async fn get_open_swap_channels(&self) -> RpcResult<OpenSwapChannels> {
		let (ethereum, bitcoin, polkadot) = tokio::try_join!(
			self.get_open_swap_channels_for_chain::<Ethereum>(None),
			self.get_open_swap_channels_for_chain::<Bitcoin>(None),
			self.get_open_swap_channels_for_chain::<Polkadot>(None),
		)?;
		Ok(OpenSwapChannels { ethereum, bitcoin, polkadot })
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

		let (tx_hash, _, _, _) = self
			.signed_pool_client
			.submit_watch(
				RuntimeCall::from(pallet_cf_funding::Call::redeem {
					amount: redeem_amount,
					address: redeem_address,
					executor: executor_address,
				}),
				false,
				true,
				None,
			)
			.await?;

		Ok(tx_hash)
	}

	async fn cancel_all_orders(
		&self,
		wait_for: Option<WaitFor>,
	) -> RpcResult<Vec<ApiWaitForResult<Vec<LimitOrRangeOrder>>>> {
		let mut orders_to_delete: Vec<CloseOrder> = vec![];

		let pool_pairs = self.client.runtime_api().cf_pools(self.client.info().best_hash)?;

		for pool in pool_pairs {
			let orders = match self.client.runtime_api().cf_pool_orders(
				self.client.info().best_hash,
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
				self.cancel_orders_batch(
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
		orders: BoundedVec<CloseOrder, ConstU32<MAX_ORDERS_DELETE>>,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<Vec<LimitOrRangeOrder>>> {
		Ok(into_api_wait_for_result(
			self.signed_pool_client
				.submit_wait_for_result(
					RuntimeCall::from(pallet_cf_pools::Call::cancel_orders_batch { orders }),
					wait_for.unwrap_or_default(),
					false,
					None,
				)
				.await?,
			collect_order_returns,
		))
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
		+ Core<B>
		+ sp_block_builder::BlockBuilder<B>
		+ sp_transaction_pool::runtime_api::TaggedTransactionQueue<B>
		+ frame_system_rpc_runtime_api::AccountNonceApi<B, AccountId, Nonce>,
{
	async fn scheduled_or_immediate(
		&self,
		call: pallet_cf_pools::Call<state_chain_runtime::Runtime>,
		dispatch_at: Option<BlockNumber>,
		wait_for: WaitFor,
	) -> RpcResult<ApiWaitForResult<Vec<LimitOrder>>> {
		Ok(into_api_wait_for_result(
			if let Some(dispatch_at) = dispatch_at {
				self.signed_pool_client
					.submit_wait_for_result(
						RuntimeCall::from(pallet_cf_pools::Call::schedule_limit_order_update {
							call: Box::new(call),
							dispatch_at,
						}),
						wait_for,
						false,
						None,
					)
					.await?
			} else {
				self.signed_pool_client
					.submit_wait_for_result(RuntimeCall::from(call), wait_for, false, None)
					.await?
			},
			collect_limit_order_returns,
		))
	}

	pub async fn get_open_swap_channels_for_chain<CH: Chain>(
		&self,
		block_hash: Option<Hash>,
	) -> RpcResult<Vec<SwapChannelInfo<CH>>>
	where
		state_chain_runtime::Runtime:
			pallet_cf_ingress_egress::Config<ChainInstanceFor<CH>, TargetChain = CH>,
	{
		let block_hash = block_hash.unwrap_or_else(|| self.client.info().finalized_hash);

		let channels = StorageQueryApi::new(&self.client)
			.collect_from_storage_map::<pallet_cf_ingress_egress::DepositChannelLookup<
			state_chain_runtime::Runtime,
			ChainInstanceFor<CH>,
		>, _, _, Vec<_>>(block_hash)?;

		let network_environment = StorageQueryApi::new(&self.client)
			.get_storage_value::<pallet_cf_environment::ChainflipNetworkEnvironment<
			state_chain_runtime::Runtime,
		>, _>(block_hash)?;

		Ok(channels
			.into_iter()
			.filter_map(|(_, DepositChannelDetails { action, deposit_channel, .. })| match action {
				pallet_cf_ingress_egress::ChannelAction::Swap { destination_asset, .. } |
				pallet_cf_ingress_egress::ChannelAction::CcmTransfer {
					destination_asset, ..
				} => Some(SwapChannelInfo {
					deposit_address: deposit_channel.address.to_humanreadable(network_environment),
					source_asset: deposit_channel.asset.into(),
					destination_asset,
				}),
				_ => None,
			})
			.collect::<Vec<_>>())
	}
}

fn into_api_wait_for_result<T>(
	from: WaitForResult,
	map_events: impl FnOnce(Vec<state_chain_runtime::RuntimeEvent>) -> T,
) -> ApiWaitForResult<T> {
	match from {
		WaitForResult::TransactionHash(tx_hash) => ApiWaitForResult::TxHash(tx_hash),
		WaitForResult::Details(details) => {
			let (tx_hash, events, ..) = details;
			ApiWaitForResult::TxDetails { tx_hash, response: map_events(events) }
		},
	}
}
