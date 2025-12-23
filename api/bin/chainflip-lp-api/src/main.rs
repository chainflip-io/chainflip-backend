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
use cf_amm_math::Price;
use cf_primitives::{
	chains::{Arbitrum, Solana},
	ApiWaitForResult, BasisPoints, BlockNumber, DcaParameters, EgressId, PriceLimits, WaitFor,
};
use cf_rpc_apis::{
	lp::LpRpcApiServer, ExtrinsicResponse, NotificationBehaviour, RpcApiError, RpcResult,
};
use cf_utilities::{
	health::{self, HealthCheckOptions},
	rpc::NumberOrHex,
	task_scope::{task_scope, Scope},
	try_parse_number_or_hex,
};
use chainflip_api::{
	self,
	lp::{
		CloseOrderJson, LimitOrRangeOrder, LimitOrder, LiquidityDepositChannelDetails, LpApi,
		OpenSwapChannels, OrderIdJson, RangeOrder, RangeOrderSizeJson, Side, Tick,
	},
	primitives::{
		chains::{assets::any::AssetMap, Bitcoin, Ethereum, Polkadot},
		AccountRole, Asset, ForeignChain, Hash,
	},
	rpc_types::{lp::SwapRequestResponse, OrderFills, RedemptionAmount},
	settings::StateChain,
	AccountId32, AddressString, BlockUpdate, ChainApi, EthereumAddress, OperatorApi,
	SignedExtrinsicApi, StateChainApi,
};
use clap::Parser;
use custom_rpc::CustomApiClient;
use futures::{stream, FutureExt, StreamExt};
use jsonrpsee::{core::async_trait, server::ServerBuilder, PendingSubscriptionSink};
use pallet_cf_pools::{CloseOrder, IncreaseOrDecrease, MAX_ORDERS_DELETE};
use sp_core::{bounded::BoundedVec, ConstU32, H256, U256};
use std::{
	ops::Range,
	path::PathBuf,
	sync::{atomic::AtomicBool, Arc},
};
use tracing::log;

pub struct RpcServerImpl {
	api: StateChainApi,
}

impl RpcServerImpl {
	pub async fn new(
		scope: &Scope<'_, anyhow::Error>,
		LPOptions { ws_endpoint, signing_key_file, .. }: LPOptions,
	) -> Result<Self, anyhow::Error> {
		Ok(Self {
			api: StateChainApi::connect(scope, StateChain { ws_endpoint, signing_key_file })
				.await?,
		})
	}
}

#[async_trait]
impl LpRpcApiServer for RpcServerImpl {
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

		Ok(self
			.api
			.lp_api()
			.request_liquidity_deposit_address(asset, wait_for_param, boost_fee)
			.await?)
	}

	async fn request_liquidity_deposit_address_v2(
		&self,
		asset: Asset,
		boost_fee: Option<BasisPoints>,
	) -> RpcResult<ExtrinsicResponse<LiquidityDepositChannelDetails>> {
		Ok(self.api.lp_api().request_liquidity_deposit_address_v2(asset, boost_fee).await?)
	}

	async fn register_liquidity_refund_address(
		&self,
		chain: ForeignChain,
		address: AddressString,
	) -> RpcResult<Hash> {
		Ok(self.api.lp_api().register_liquidity_refund_address(chain, address).await?)
	}

	async fn withdraw_asset(
		&self,
		amount: NumberOrHex,
		asset: Asset,
		destination_address: AddressString,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<EgressId>> {
		Ok(self
			.api
			.lp_api()
			.withdraw_asset(
				try_parse_number_or_hex(amount)?,
				asset,
				destination_address,
				wait_for.unwrap_or_default(),
			)
			.await?)
	}

	async fn transfer_asset(
		&self,
		amount: U256,
		asset: Asset,
		destination_account: AccountId32,
	) -> RpcResult<H256> {
		Ok(self
			.api
			.lp_api()
			.transfer_asset(
				amount.try_into().map_err(|_| anyhow!("Failed to convert amount to u128"))?,
				asset,
				destination_account,
			)
			.await?)
	}

	/// Returns a list of all assets and their free balance in json format
	async fn free_balances(&self) -> RpcResult<AssetMap<U256>> {
		Ok(self
			.api
			.state_chain_client
			.base_rpc_client
			.raw_rpc_client
			.cf_free_balances(
				self.api.state_chain_client.account_id(),
				Some(self.api.state_chain_client.latest_finalized_block().hash),
			)
			.await?)
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
		Ok(self
			.api
			.lp_api()
			.update_range_order(
				base_asset,
				quote_asset,
				id.try_into()?,
				tick_range,
				size_change.try_map(|size| size.try_into())?,
				wait_for.unwrap_or_default(),
			)
			.await?)
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
		Ok(self
			.api
			.lp_api()
			.set_range_order(
				base_asset,
				quote_asset,
				id.try_into()?,
				tick_range,
				size.try_into()?,
				wait_for.unwrap_or_default(),
			)
			.await?)
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
		Ok(self
			.api
			.lp_api()
			.update_limit_order(
				base_asset,
				quote_asset,
				side,
				id.try_into()?,
				tick,
				amount_change.try_map(try_parse_number_or_hex)?,
				dispatch_at,
				wait_for.unwrap_or_default(),
			)
			.await?)
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
		Ok(self
			.api
			.lp_api()
			.set_limit_order(
				base_asset,
				quote_asset,
				side,
				id.try_into()?,
				tick,
				try_parse_number_or_hex(sell_amount)?,
				dispatch_at,
				wait_for.unwrap_or_default(),
				close_order_at,
			)
			.await?)
	}

	/// Returns the tx hash that the account role was set
	async fn register_account(&self) -> RpcResult<Hash> {
		Ok(self
			.api
			.operator_api()
			.register_account_role(AccountRole::LiquidityProvider)
			.await?)
	}

	async fn get_open_swap_channels(&self, at: Option<Hash>) -> RpcResult<OpenSwapChannels> {
		let api = self.api.query_api();

		let (ethereum, bitcoin, polkadot, arbitrum, solana) = tokio::try_join!(
			api.get_open_swap_channels::<Ethereum>(at),
			api.get_open_swap_channels::<Bitcoin>(at),
			api.get_open_swap_channels::<Polkadot>(at),
			api.get_open_swap_channels::<Arbitrum>(at),
			api.get_open_swap_channels::<Solana>(at),
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

		Ok(self
			.api
			.operator_api()
			.request_redemption(redeem_amount, redeem_address, executor_address)
			.await?
			.tx_hash)
	}

	async fn subscribe_order_fills(
		&self,
		pending_sink: PendingSubscriptionSink,
		notification_behaviour: Option<NotificationBehaviour>,
	) {
		// pipe results from custom-rpc subscription
		match self.api.raw_client().cf_subscribe_lp_order_fills(notification_behaviour).await {
			Ok(subscription) => {
				let stream = stream::unfold(subscription, move |mut sub| async move {
					match sub.next().await {
						Some(Ok(block_update)) => Some((block_update, sub)),
						_ => None,
					}
				})
				.boxed();

				tokio::spawn(async move {
					sc_rpc::utils::pipe_from_stream(pending_sink, stream).await;
				});
			},
			Err(e) => {
				pending_sink.reject(RpcApiError::ClientError(e)).await;
			},
		}
	}

	async fn order_fills(&self, at: Option<Hash>) -> RpcResult<BlockUpdate<OrderFills>> {
		self.api
			.raw_client()
			.cf_lp_get_order_fills(at)
			.await
			.map_err(RpcApiError::ClientError)
	}

	async fn cancel_all_orders(
		&self,
		wait_for: Option<WaitFor>,
	) -> RpcResult<Vec<ApiWaitForResult<Vec<LimitOrRangeOrder>>>> {
		let mut orders_to_delete: Vec<CloseOrder> = vec![];
		let pool_pairs = self
			.api
			.state_chain_client
			.base_rpc_client
			.raw_rpc_client
			.cf_available_pools(None)
			.await?;
		for pool in pool_pairs {
			let orders = self
				.api
				.state_chain_client
				.base_rpc_client
				.raw_rpc_client
				.cf_pool_orders(
					pool.base,
					pool.quote,
					Some(self.api.state_chain_client.account_id()),
					None,
					None,
				)
				.await?;
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
				self.api
					.lp_api()
					.cancel_orders_batch(
						BoundedVec::<_, ConstU32<MAX_ORDERS_DELETE>>::try_from(
							order_chunk.to_vec(),
						)
						.expect("Guaranteed by `chunk` method."),
						wait_for.unwrap_or_default(),
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
		Ok(self
			.api
			.lp_api()
			.cancel_orders_batch(
				orders
					.into_iter()
					.map(TryInto::try_into)
					.collect::<Result<Vec<_>, _>>()?
					.try_into()
					.expect("Impossible to fail, given the same MAX_ORDERS_DELETE"),
				wait_for.unwrap_or_default(),
			)
			.await?)
	}

	async fn schedule_swap(
		&self,
		amount: NumberOrHex,
		input_asset: Asset,
		output_asset: Asset,
		retry_duration: BlockNumber,
		price_limits: PriceLimits<Price>,
		dca_params: Option<DcaParameters>,
		wait_for: Option<WaitFor>,
	) -> RpcResult<ApiWaitForResult<SwapRequestResponse>> {
		Ok(self
			.api
			.lp_api()
			.schedule_swap(
				try_parse_number_or_hex(amount)?,
				input_asset,
				output_asset,
				retry_duration,
				price_limits,
				dca_params,
				wait_for.unwrap_or_default(),
			)
			.await?
			.map_details(Into::into))
	}
}

#[derive(Parser, Debug, Clone, Default)]
#[clap(version = env!("SUBSTRATE_CLI_IMPL_VERSION"), short_flag = 'v')]
pub struct LPOptions {
	#[clap(
		long = "port",
		default_value = "80",
		help = "The port number on which the LP server will listen for connections. Use 0 to assign a random port."
	)]
	pub port: u16,
	#[clap(
		long = "state_chain.ws_endpoint",
		default_value = "ws://localhost:9944",
		help = "The state chain node's RPC endpoint."
	)]
	pub ws_endpoint: String,
	#[clap(
		long = "state_chain.signing_key_file",
		default_value = "/etc/chainflip/keys/signing_key_file",
		help = "A path to a file that contains the LP's secret key for signing extrinsics."
	)]
	pub signing_key_file: PathBuf,
	#[clap(flatten)]
	pub health_check: HealthCheckOptions,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let opts = LPOptions::parse();
	chainflip_api::use_chainflip_account_id_encoding();
	tracing_subscriber::FmtSubscriber::builder()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.try_init()
		.expect("setting default subscriber failed");

	assert!(
		opts.signing_key_file.exists(),
		"No signing_key_file found at {}",
		opts.signing_key_file.to_string_lossy()
	);

	task_scope(|scope| {
		async move {
			// initialize healthcheck endpoint
			let has_completed_initialising = Arc::new(AtomicBool::new(false));
			health::start_if_configured(
				scope,
				&opts.health_check,
				has_completed_initialising.clone(),
			)
			.await?;

			let server = ServerBuilder::default().build(format!("0.0.0.0:{}", opts.port)).await?;
			let server_addr = server.local_addr()?;
			let server = server.start(RpcServerImpl::new(scope, opts).await?.into_rpc());

			log::info!("🎙 Server is listening on {server_addr}.");

			// notify healthcheck completed
			has_completed_initialising.store(true, std::sync::atomic::Ordering::Relaxed);

			server.stopped().await;
			Ok(())
		}
		.boxed()
	})
	.await
}
