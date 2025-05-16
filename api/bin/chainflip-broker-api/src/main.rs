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

use cf_chains::CcmChannelMetadataUnchecked;
use cf_rpc_apis::{
	broker::{
		BrokerRpcApiServer, ChannelRefundParametersRpc, DcaParameters, GetOpenDepositChannelsQuery,
		RpcBytes, SwapDepositAddress, TransactionInId, VaultSwapExtraParametersRpc,
		VaultSwapInputRpc, WithdrawFeesDetail,
	},
	RefundParametersRpc, RpcApiError, RpcResult,
};
use cf_utilities::{
	health::{self, HealthCheckOptions},
	task_scope::{task_scope, Scope},
};
use chainflip_api::{
	self,
	primitives::{
		state_chain_runtime::runtime_apis::{ChainAccounts, VaultAddresses, VaultSwapDetails},
		AccountRole, AffiliateDetails, Affiliates, Asset, BasisPoints,
	},
	rpc_types::H256,
	settings::StateChain,
	AccountId32, AddressString, BrokerApi, ChannelActionType, ChannelId, DepositMonitorApi,
	EthereumAddress, OperatorApi, SignedExtrinsicApi, StateChainApi,
};
use clap::Parser;
use custom_rpc::CustomApiClient;
use futures::{stream, FutureExt, StreamExt};
use jsonrpsee::{core::async_trait, server::ServerBuilder, PendingSubscriptionSink};
use std::{
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
		BrokerOptions { ws_endpoint, signing_key_file, .. }: BrokerOptions,
	) -> Result<Self, anyhow::Error> {
		Ok(Self {
			api: StateChainApi::connect(scope, StateChain { ws_endpoint, signing_key_file })
				.await?,
		})
	}
}

#[async_trait]
impl BrokerRpcApiServer for RpcServerImpl {
	async fn register_account(&self) -> RpcResult<String> {
		Ok(self
			.api
			.operator_api()
			.register_account_role(AccountRole::Broker)
			.await
			.map(|tx_hash| format!("{tx_hash:#x}"))?)
	}

	async fn request_swap_deposit_address(
		&self,
		source_asset: Asset,
		destination_asset: Asset,
		destination_address: AddressString,
		broker_commission: BasisPoints,
		channel_metadata: Option<CcmChannelMetadataUnchecked>,
		boost_fee: Option<BasisPoints>,
		affiliate_fees: Option<Affiliates<AccountId32>>,
		refund_parameters: RefundParametersRpc,
		dca_parameters: Option<DcaParameters>,
		wait_for_finality: Option<bool>,
	) -> RpcResult<SwapDepositAddress> {
		Ok(self
			.api
			.broker_api()
			.request_swap_deposit_address(
				source_asset,
				destination_asset,
				destination_address,
				broker_commission,
				channel_metadata,
				boost_fee,
				affiliate_fees,
				refund_parameters,
				dca_parameters,
				wait_for_finality,
			)
			.await?)
	}

	async fn withdraw_fees(
		&self,
		asset: Asset,
		destination_address: AddressString,
	) -> RpcResult<WithdrawFeesDetail> {
		Ok(self.api.broker_api().withdraw_fees(asset, destination_address).await?)
	}

	async fn request_swap_parameter_encoding(
		&self,
		source_asset: Asset,
		destination_asset: Asset,
		destination_address: AddressString,
		broker_commission: BasisPoints,
		extra_parameters: VaultSwapExtraParametersRpc,
		channel_metadata: Option<CcmChannelMetadataUnchecked>,
		boost_fee: Option<BasisPoints>,
		affiliate_fees: Option<Affiliates<AccountId32>>,
		dca_parameters: Option<DcaParameters>,
	) -> RpcResult<VaultSwapDetails<AddressString>> {
		Ok(self
			.api
			.raw_client()
			.cf_request_swap_parameter_encoding(
				self.api.state_chain_client.account_id(),
				source_asset,
				destination_asset,
				destination_address,
				broker_commission,
				extra_parameters,
				channel_metadata,
				boost_fee,
				affiliate_fees,
				dca_parameters,
				None,
			)
			.await?)
	}

	async fn decode_vault_swap_parameter(
		&self,
		vault_swap: VaultSwapDetails<AddressString>,
	) -> RpcResult<VaultSwapInputRpc> {
		Ok(self
			.api
			.raw_client()
			.cf_decode_vault_swap_parameter(
				self.api.state_chain_client.account_id(),
				vault_swap,
				None,
			)
			.await?)
	}

	async fn encode_cf_parameters(
		&self,
		source_asset: Asset,
		destination_asset: Asset,
		destination_address: AddressString,
		broker_commission: BasisPoints,
		refund_parameters: ChannelRefundParametersRpc,
		channel_metadata: Option<CcmChannelMetadataUnchecked>,
		boost_fee: Option<BasisPoints>,
		affiliate_fees: Option<Affiliates<AccountId32>>,
		dca_parameters: Option<DcaParameters>,
	) -> RpcResult<RpcBytes> {
		Ok(self
			.api
			.raw_client()
			.cf_encode_cf_parameters(
				self.api.state_chain_client.account_id(),
				source_asset,
				destination_asset,
				destination_address,
				broker_commission,
				refund_parameters,
				channel_metadata,
				boost_fee,
				affiliate_fees,
				dca_parameters,
				None,
			)
			.await?)
	}

	async fn mark_transaction_for_rejection(&self, tx_id: TransactionInId) -> RpcResult<()> {
		self.api
			.deposit_monitor_api()
			.mark_transaction_for_rejection(tx_id)
			.await
			.map_err(RpcApiError::Other)?;
		Ok(())
	}

	async fn get_open_deposit_channels(
		&self,
		query: GetOpenDepositChannelsQuery,
	) -> RpcResult<ChainAccounts> {
		let account_id = match query {
			GetOpenDepositChannelsQuery::All => None,
			GetOpenDepositChannelsQuery::Mine => Some(self.api.state_chain_client.account_id()),
		};

		self.api
			.raw_client()
			.cf_get_open_deposit_channels(account_id, None)
			.await
			.map_err(RpcApiError::ClientError)
	}

	async fn all_open_deposit_channels(
		&self,
	) -> RpcResult<Vec<(AccountId32, ChannelActionType, ChainAccounts)>> {
		self.api
			.raw_client()
			.cf_all_open_deposit_channels(None)
			.await
			.map_err(RpcApiError::ClientError)
	}

	async fn subscribe_transaction_screening_events(&self, pending_sink: PendingSubscriptionSink) {
		// pipe results through from custom-rpc subscription
		match self.api.raw_client().cf_subscribe_transaction_screening_events().await {
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

	async fn open_private_btc_channel(&self) -> RpcResult<ChannelId> {
		Ok(self.api.broker_api().open_private_btc_channel().await?)
	}

	async fn close_private_btc_channel(&self) -> RpcResult<ChannelId> {
		Ok(self.api.broker_api().close_private_btc_channel().await?)
	}

	async fn register_affiliate(
		&self,
		withdrawal_address: EthereumAddress,
	) -> RpcResult<AccountId32> {
		Ok(self.api.broker_api().register_affiliate(withdrawal_address).await?)
	}

	async fn get_affiliates(
		&self,
		affiliate: Option<AccountId32>,
	) -> RpcResult<Vec<(AccountId32, AffiliateDetails)>> {
		Ok(self
			.api
			.raw_client()
			.cf_affiliate_details(self.api.state_chain_client.account_id(), affiliate, None)
			.await?)
	}

	async fn affiliate_withdrawal_request(
		&self,
		affiliate_account_id: AccountId32,
	) -> RpcResult<WithdrawFeesDetail> {
		Ok(self.api.broker_api().affiliate_withdrawal_request(affiliate_account_id).await?)
	}

	async fn vault_addresses(&self) -> RpcResult<VaultAddresses> {
		Ok(self.api.raw_client().cf_vault_addresses(None).await?)
	}

	async fn set_vault_swap_minimum_broker_fee(
		&self,
		minimum_fee_bps: BasisPoints,
	) -> RpcResult<H256> {
		Ok(self.api.broker_api().set_vault_swap_minimum_broker_fee(minimum_fee_bps).await?)
	}
}

#[derive(Parser, Debug, Clone, Default)]
#[clap(version = env!("SUBSTRATE_CLI_IMPL_VERSION"))]
pub struct BrokerOptions {
	#[clap(
		long = "port",
		default_value = "80",
		help = "The port number on which the broker will listen for connections. Use 0 to assign a random port."
	)]
	pub port: u16,
	#[clap(
		long = "max_connections",
		default_value = "100",
		help = "The maximum number of concurrent websocket connections to accept."
	)]
	pub max_connections: u32,
	#[clap(
		long = "state_chain.ws_endpoint",
		default_value = "ws://localhost:9944",
		help = "The state chain node's RPC endpoint."
	)]
	pub ws_endpoint: String,
	#[clap(
		long = "state_chain.signing_key_file",
		default_value = "/etc/chainflip/keys/signing_key_file",
		help = "A path to a file that contains the broker's secret key for signing extrinsics."
	)]
	pub signing_key_file: PathBuf,
	#[clap(flatten)]
	pub health_check: HealthCheckOptions,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let opts = BrokerOptions::parse();
	chainflip_api::use_chainflip_account_id_encoding();
	tracing_subscriber::FmtSubscriber::builder()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.try_init()
		.expect("setting default subscriber failed");

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

			let server = ServerBuilder::default()
				.max_connections(opts.max_connections)
				.build(format!("0.0.0.0:{}", opts.port))
				.await?;
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
