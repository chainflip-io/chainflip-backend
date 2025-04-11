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
	pool_client::SignedPoolClient,
	CfApiError, RpcResult,
};
pub use cf_chains::eth::Address as EthereumAddress;
use cf_chains::{
	address::AddressString, CcmChannelMetadata, ChannelRefundParameters, RefundParametersRpc,
	VaultSwapExtraParametersRpc,
};
use cf_node_client::{
	extract_from_first_matching_event, subxt_state_chain_config::cf_static_runtime, ExtrinsicData,
};
use cf_primitives::{Affiliates, Asset, BasisPoints, ChannelId, DcaParameters};
use cf_rpc_types::broker::{
	GetOpenDepositChannelsQuery, SwapDepositAddress, TransactionInId, WithdrawFeesDetail,
};
use jsonrpsee::{core::async_trait, proc_macros::rpc, PendingSubscriptionSink};
use pallet_cf_swapping::AffiliateDetails;
use sc_client_api::{
	blockchain::HeaderMetadata, Backend, BlockBackend, BlockchainEvents, ExecutorProvider,
	HeaderBackend, StorageProvider,
};
use sc_transaction_pool::FullPool;
use sp_api::CallApiAt;
use sp_core::crypto::AccountId32;
use sp_runtime::traits::Block as BlockT;
use state_chain_runtime::{
	chainflip::BlockUpdate,
	runtime_apis::{
		ChainAccounts, CustomRuntimeApi, TransactionScreeningEvents, VaultAddresses,
		VaultSwapDetails,
	},
	AccountId, Nonce, RuntimeCall,
};
use std::sync::Arc;

pub mod broker_crypto {
	use sp_application_crypto::{app_crypto, sr25519, KeyTypeId};
	/// Broker Key Type ID used to store the key on state chain node keystore
	pub const BROKER_KEY_TYPE_ID: KeyTypeId = KeyTypeId(*b"brok");

	app_crypto!(sr25519, BROKER_KEY_TYPE_ID);
}

#[rpc(server, client, namespace = "broker")]
pub trait BrokerSignedApi {
	#[method(name = "register_account", aliases = ["broker_registerAccount"])]
	async fn register_account(&self) -> RpcResult<String>;

	#[method(name = "request_swap_deposit_address", aliases = ["broker_requestSwapDepositAddress"])]
	async fn request_swap_deposit_address(
		&self,
		source_asset: Asset,
		destination_asset: Asset,
		destination_address: AddressString,
		broker_commission: BasisPoints,
		channel_metadata: Option<CcmChannelMetadata>,
		boost_fee: Option<BasisPoints>,
		affiliate_fees: Option<Affiliates<AccountId32>>,
		refund_parameters: RefundParametersRpc,
		dca_parameters: Option<DcaParameters>,
	) -> RpcResult<SwapDepositAddress>;

	#[method(name = "withdraw_fees", aliases = ["broker_withdrawFees"])]
	async fn withdraw_fees(
		&self,
		asset: Asset,
		destination_address: AddressString,
	) -> RpcResult<WithdrawFeesDetail>;

	#[method(name = "request_swap_parameter_encoding", aliases = ["broker_requestSwapParameterEncoding"])]
	async fn request_swap_parameter_encoding(
		&self,
		source_asset: Asset,
		destination_asset: Asset,
		destination_address: AddressString,
		broker_commission: BasisPoints,
		extra_parameters: VaultSwapExtraParametersRpc,
		channel_metadata: Option<CcmChannelMetadata>,
		boost_fee: Option<BasisPoints>,
		affiliate_fees: Option<Affiliates<AccountId32>>,
		dca_parameters: Option<DcaParameters>,
	) -> RpcResult<VaultSwapDetails<AddressString>>;

	#[method(name = "mark_transaction_for_rejection", aliases = ["broker_MarkTransactionForRejection"])]
	async fn mark_transaction_for_rejection(&self, tx_id: TransactionInId) -> RpcResult<()>;

	#[method(name = "get_open_deposit_channels", aliases = ["broker_getOpenDepositChannels"])]
	async fn get_open_deposit_channels(
		&self,
		query: GetOpenDepositChannelsQuery,
	) -> RpcResult<ChainAccounts>;

	#[subscription(name = "subscribe_transaction_screening_events", item = BlockUpdate<TransactionScreeningEvents>)]
	async fn subscribe_transaction_screening_events(&self);

	#[method(name = "open_private_btc_channel", aliases = ["broker_openPrivateBtcChannel"])]
	async fn open_private_btc_channel(&self) -> RpcResult<ChannelId>;

	#[method(name = "close_private_btc_channel", aliases = ["broker_closePrivateBtcChannel"])]
	async fn close_private_btc_channel(&self) -> RpcResult<ChannelId>;

	#[method(name = "register_affiliate", aliases = ["broker_registerAffiliate"])]
	async fn register_affiliate(
		&self,
		withdrawal_address: EthereumAddress,
	) -> RpcResult<AccountId32>;

	#[method(name = "get_affiliates", aliases = ["broker_getAffiliates"])]
	async fn get_affiliates(
		&self,
		affiliate: Option<AccountId32>,
	) -> RpcResult<Vec<(AccountId32, AffiliateDetails)>>;

	#[method(name = "affiliate_withdrawal_request", aliases = ["broker_affiliateWithdrawalRequest"])]
	async fn affiliate_withdrawal_request(
		&self,
		affiliate_account_id: AccountId32,
	) -> RpcResult<WithdrawFeesDetail>;

	#[method(name = "get_vault_addresses", aliases = ["broker_getVaultAddresses"])]
	async fn vault_addresses(&self) -> RpcResult<VaultAddresses>;

	#[method(name = "set_vault_swap_minimum_broker_fee", aliases = ["broker_setVaultSwapMinimumBrokerFee"])]
	async fn set_vault_swap_minimum_broker_fee(
		&self,
		minimum_fee_bps: BasisPoints,
	) -> RpcResult<()>;
}

/// A Broker signed RPC extension for the state chain node.
pub struct BrokerSignedRpc<C, B, BE>
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

impl<C, B, BE> BrokerSignedRpc<C, B, BE>
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
}

#[async_trait]
impl<C, B, BE> BrokerSignedApiServer for BrokerSignedRpc<C, B, BE>
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
	async fn register_account(&self) -> RpcResult<String> {
		let ExtrinsicData { tx_hash, .. } = self
			.signed_pool_client
			.submit_watch_dynamic(
				RuntimeCall::from(pallet_cf_swapping::Call::register_as_broker {}),
				false,
				true,
			)
			.await?;

		Ok(format!("{:#x}", tx_hash))
	}

	async fn request_swap_deposit_address(
		&self,
		source_asset: Asset,
		destination_asset: Asset,
		destination_address: AddressString,
		broker_commission: BasisPoints,
		channel_metadata: Option<CcmChannelMetadata>,
		boost_fee: Option<BasisPoints>,
		affiliate_fees: Option<Affiliates<AccountId32>>,
		refund_parameters: RefundParametersRpc,
		dca_parameters: Option<DcaParameters>,
	) -> RpcResult<SwapDepositAddress> {
		let ExtrinsicData { events, header, .. } = self
			.signed_pool_client
			.submit_watch_dynamic(
				RuntimeCall::from(
					pallet_cf_swapping::Call::request_swap_deposit_address_with_affiliates {
						source_asset,
						destination_asset,
						destination_address: destination_address
							.try_parse_to_encoded_address(destination_asset.into())?,
						broker_commission,
						channel_metadata,
						boost_fee: boost_fee.unwrap_or_default(),
						affiliate_fees: affiliate_fees.unwrap_or_default(),
						refund_parameters: refund_parameters.try_map_address(|addr| {
							addr.try_parse_to_encoded_address(source_asset.into())
						})?,
						dca_parameters,
					},
				),
				false,
				true,
			)
			.await?;

		Ok(extract_from_first_matching_event!(
			events,
			cf_static_runtime::swapping::events::SwapDepositAddressReady,
			{
				deposit_address,
				channel_id,
				source_chain_expiry_block,
				channel_opening_fee,
				refund_parameters
			},
			SwapDepositAddress {
				address: AddressString::from_encoded_address(deposit_address.0),
				issued_block: header.number,
				channel_id,
				source_chain_expiry_block: source_chain_expiry_block.into(),
				channel_opening_fee: channel_opening_fee.into(),
				refund_parameters: ChannelRefundParameters::from(refund_parameters)
				.map_address(|refund_address| {
					AddressString::from_encoded_address(&refund_address.0)
				}),
			}
		)?)
	}

	async fn withdraw_fees(
		&self,
		asset: Asset,
		destination_address: AddressString,
	) -> RpcResult<WithdrawFeesDetail> {
		let ExtrinsicData { tx_hash, events, .. } = self
			.signed_pool_client
			.submit_watch_dynamic(
				RuntimeCall::from(pallet_cf_swapping::Call::withdraw {
					asset,
					destination_address: destination_address
						.try_parse_to_encoded_address(asset.into())
						.map_err(anyhow::Error::msg)?,
				}),
				false,
				false,
			)
			.await?;

		Ok(extract_from_first_matching_event!(
			events,
			cf_static_runtime::swapping::events::WithdrawalRequested,
			{
				egress_amount,
				egress_fee,
				destination_address,
				egress_id
			},
			WithdrawFeesDetail {
				tx_hash,
				egress_id: (egress_id.0.0, egress_id.1),
				egress_amount: egress_amount.into(),
				egress_fee: egress_fee.into(),
				destination_address: AddressString::from_encoded_address(destination_address.0),
			}
		)?)
	}

	// This is also defined in custom-rpc. // TODO: try to define only in one place
	async fn request_swap_parameter_encoding(
		&self,
		source_asset: Asset,
		destination_asset: Asset,
		destination_address: AddressString,
		broker_commission: BasisPoints,
		extra_parameters: VaultSwapExtraParametersRpc,
		channel_metadata: Option<CcmChannelMetadata>,
		boost_fee: Option<BasisPoints>,
		affiliate_fees: Option<Affiliates<AccountId32>>,
		dca_parameters: Option<DcaParameters>,
	) -> RpcResult<VaultSwapDetails<AddressString>> {
		Ok(self
			.rpc_backend
			.client
			.runtime_api()
			.cf_request_swap_parameter_encoding(
				self.rpc_backend.client.info().best_hash,
				self.signed_pool_client.account_id(),
				source_asset,
				destination_asset,
				destination_address.try_parse_to_encoded_address(destination_asset.into())?,
				broker_commission,
				extra_parameters.try_into_encoded_params(source_asset.into())?,
				channel_metadata,
				boost_fee.unwrap_or_default(),
				affiliate_fees.unwrap_or_default(),
				dca_parameters,
			)??
			.map_btc_address(Into::into))
	}

	async fn mark_transaction_for_rejection(&self, tx_id: TransactionInId) -> RpcResult<()> {
		self.signed_pool_client
			.submit_watch_dynamic(
				match tx_id {
					TransactionInId::Bitcoin(tx_id) => RuntimeCall::BitcoinIngressEgress(
						pallet_cf_ingress_egress::Call::mark_transaction_for_rejection { tx_id },
					),
					TransactionInId::Ethereum(tx_id) => RuntimeCall::EthereumIngressEgress(
						pallet_cf_ingress_egress::Call::mark_transaction_for_rejection { tx_id },
					),
					TransactionInId::Arbitrum(tx_id) => RuntimeCall::ArbitrumIngressEgress(
						pallet_cf_ingress_egress::Call::mark_transaction_for_rejection { tx_id },
					),
				},
				false,
				true,
			)
			.await?;
		Ok(())
	}

	async fn get_open_deposit_channels(
		&self,
		query: GetOpenDepositChannelsQuery,
	) -> RpcResult<ChainAccounts> {
		let account_id = match query {
			GetOpenDepositChannelsQuery::All => None,
			GetOpenDepositChannelsQuery::Mine => Some(self.signed_pool_client.account_id()),
		};

		self.rpc_backend
			.client
			.runtime_api()
			.cf_get_open_deposit_channels(self.rpc_backend.client.info().best_hash, account_id)
			.map_err(CfApiError::RuntimeApiError)
	}

	async fn subscribe_transaction_screening_events(&self, pending_sink: PendingSubscriptionSink) {
		self.rpc_backend
			.new_subscription(
				NotificationBehaviour::Finalized,
				false,
				true,
				pending_sink,
				move |client, hash| {
					Ok((*client.runtime_api()).cf_transaction_screening_events(hash)?)
				},
			)
			.await;
	}

	async fn open_private_btc_channel(&self) -> RpcResult<ChannelId> {
		let ExtrinsicData { events, .. } = self
			.signed_pool_client
			.submit_watch_dynamic(
				RuntimeCall::from(pallet_cf_swapping::Call::open_private_btc_channel {}),
				false,
				true,
			)
			.await?;

		Ok(extract_from_first_matching_event!(
			events,
			cf_static_runtime::swapping::events::PrivateBrokerChannelOpened,
			{ channel_id },
			channel_id
		)?)
	}

	async fn close_private_btc_channel(&self) -> RpcResult<ChannelId> {
		let ExtrinsicData { events, .. } = self
			.signed_pool_client
			.submit_watch_dynamic(
				RuntimeCall::from(pallet_cf_swapping::Call::close_private_btc_channel {}),
				false,
				true,
			)
			.await?;

		Ok(extract_from_first_matching_event!(
			events,
			cf_static_runtime::swapping::events::PrivateBrokerChannelClosed,
			{ channel_id },
			channel_id
		)?)
	}

	async fn register_affiliate(
		&self,
		withdrawal_address: EthereumAddress,
	) -> RpcResult<AccountId32> {
		let ExtrinsicData { events, .. } = self
			.signed_pool_client
			.submit_watch_dynamic(
				RuntimeCall::from(pallet_cf_swapping::Call::register_affiliate {
					withdrawal_address,
				}),
				false,
				true,
			)
			.await?;

		Ok(extract_from_first_matching_event!(
			events,
			cf_static_runtime::swapping::events::AffiliateRegistration,
			{ affiliate_id },
			AccountId32::from(affiliate_id.0)
		)?)
	}

	async fn get_affiliates(
		&self,
		affiliate: Option<AccountId32>,
	) -> RpcResult<Vec<(AccountId32, AffiliateDetails)>> {
		self.rpc_backend
			.client
			.runtime_api()
			.cf_affiliate_details(
				self.rpc_backend.client.info().best_hash,
				self.signed_pool_client.account_id(),
				affiliate,
			)
			.map_err(CfApiError::RuntimeApiError)
	}

	async fn affiliate_withdrawal_request(
		&self,
		affiliate_account_id: AccountId32,
	) -> RpcResult<WithdrawFeesDetail> {
		let ExtrinsicData { tx_hash, events, .. } = self
			.signed_pool_client
			.submit_watch_dynamic(
				RuntimeCall::from(pallet_cf_swapping::Call::affiliate_withdrawal_request {
					affiliate_account_id,
				}),
				false,
				true,
			)
			.await?;

		Ok(extract_from_first_matching_event!(
			events,
			cf_static_runtime::swapping::events::WithdrawalRequested,
			{ egress_amount, egress_fee, destination_address, egress_id },
			WithdrawFeesDetail {
				tx_hash,
				egress_id: (egress_id.0.0, egress_id.1),
				egress_amount: egress_amount.into(),
				egress_fee: egress_fee.into(),
				destination_address: AddressString::from_encoded_address(destination_address.0),
			}
		)?)
	}

	async fn vault_addresses(&self) -> RpcResult<VaultAddresses> {
		self.rpc_backend
			.client
			.runtime_api()
			.cf_vault_addresses(self.rpc_backend.client.info().best_hash)
			.map_err(CfApiError::RuntimeApiError)
	}

	async fn set_vault_swap_minimum_broker_fee(
		&self,
		minimum_fee_bps: BasisPoints,
	) -> RpcResult<()> {
		self.signed_pool_client
			.submit_watch_dynamic(
				RuntimeCall::from(pallet_cf_swapping::Call::set_vault_swap_minimum_broker_fee {
					minimum_fee_bps,
				}),
				false,
				true,
			)
			.await?;
		Ok(())
	}
}
