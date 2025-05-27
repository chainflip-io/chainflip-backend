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
	pool_client::{is_transaction_status_error, PoolClientError, SignedPoolClient},
	CfApiError,
};
pub use cf_chains::eth::Address as EthereumAddress;
use cf_chains::{address::AddressString, CcmChannelMetadataUnchecked, ChannelRefundParameters};
use cf_node_client::{
	extract_from_first_matching_event, subxt_state_chain_config::cf_static_runtime, ExtrinsicData,
};
use cf_primitives::{Affiliates, Asset, BasisPoints, ChannelId, ForeignChain};
use cf_rpc_apis::{
	broker::{
		try_into_refund_parameters_encoded, try_into_swap_extra_params_encoded,
		vault_swap_input_encoded_to_rpc, BrokerRpcApiServer, ChannelRefundParametersRpc,
		DcaParameters, GetOpenDepositChannelsQuery, RpcBytes, SwapDepositAddress, TransactionInId,
		VaultSwapExtraParametersRpc, VaultSwapInputRpc, WithdrawFeesDetail,
	},
	RefundParametersRpc, RpcResult, H256,
};
use futures::StreamExt;
use jsonrpsee::{core::async_trait, PendingSubscriptionSink};
use pallet_cf_swapping::AffiliateDetails;
use sc_client_api::{
	blockchain::HeaderMetadata, Backend, BlockBackend, BlockchainEvents, ExecutorProvider,
	HeaderBackend, StorageProvider,
};
use sc_transaction_pool::FullPool;
use sc_transaction_pool_api::{TransactionStatus, TxIndex};
use sp_api::CallApiAt;
use sp_core::crypto::AccountId32;
use sp_runtime::traits::Block as BlockT;
use state_chain_runtime::{
	runtime_apis::{
		ChainAccounts, ChannelActionType, CustomRuntimeApi, VaultAddresses, VaultSwapDetails,
	},
	AccountId, Hash, Nonce, RuntimeCall,
};
use std::sync::Arc;

pub mod broker_crypto {
	use sp_application_crypto::{app_crypto, sr25519, KeyTypeId};
	/// Broker Key Type ID used to store the key on state chain node keystore
	pub const BROKER_KEY_TYPE_ID: KeyTypeId = KeyTypeId(*b"brok");

	app_crypto!(sr25519, BROKER_KEY_TYPE_ID);
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

	async fn extract_swap_deposit_address(
		&self,
		block_hash: Hash,
		tx_index: TxIndex,
	) -> RpcResult<SwapDepositAddress> {
		let ExtrinsicData { events, header, .. } = self
			.signed_pool_client
			.get_extrinsic_data_dynamic(block_hash, tx_index)
			.await
			.map_err(CfApiError::from)?;

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
		)
		.map_err(CfApiError::from)?)
	}
}

#[async_trait]
impl<C, B, BE> BrokerRpcApiServer for BrokerSignedRpc<C, B, BE>
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
			.await
			.map_err(CfApiError::from)?;

		Ok(format!("{:#x}", tx_hash))
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
		_wait_for_finality: Option<bool>,
	) -> RpcResult<SwapDepositAddress> {
		let mut status_stream = self
			.signed_pool_client
			.submit_watch(
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
				true,
			)
			.await
			.map_err(CfApiError::from)?;

		// Get the pre-allocated channels from the previous finalized block
		let source_chain: ForeignChain = source_asset.into();
		let pre_allocated_channels = self.rpc_backend.with_runtime_api(
			Some(self.rpc_backend.client.info().finalized_hash),
			|api, hash| {
				api.cf_get_preallocated_deposit_channels(
					hash,
					self.signed_pool_client.account_id(),
					source_chain,
				)
			},
		)?;

		while let Some(status) = status_stream.next().await {
			match status {
				TransactionStatus::InBlock((block_hash, tx_index)) => {
					let swap_deposit_address =
						self.extract_swap_deposit_address(block_hash, tx_index).await?;

					// If the extracted deposit channel was pre-allocated to this broker
					// in the previous finalized block, we can return it immediately.
					// Otherwise, we need to wait for the transaction to be finalized.
					if pre_allocated_channels
						.iter()
						.any(|channel_id| channel_id == &swap_deposit_address.channel_id)
					{
						return Ok(swap_deposit_address);
					}
				},
				TransactionStatus::Finalized((block_hash, tx_index)) =>
					return self.extract_swap_deposit_address(block_hash, tx_index).await,
				_ =>
					if let Some(e) = is_transaction_status_error(&status) {
						Err(CfApiError::from(e))?;
					},
			}
		}
		Err(CfApiError::from(PoolClientError::UnexpectedEndOfStream))?
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
			.await
			.map_err(CfApiError::from)?;

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
		)
		.map_err(CfApiError::from)?)
	}

	// This is also defined in custom-rpc. // TODO: try to define only in one place
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
				try_into_swap_extra_params_encoded(extra_parameters, source_asset.into())?,
				channel_metadata,
				boost_fee.unwrap_or_default(),
				affiliate_fees.unwrap_or_default(),
				dca_parameters,
			)
			.map_err(CfApiError::from)?
			.map_err(CfApiError::from)?
			.map_btc_address(Into::into))
	}

	async fn decode_vault_swap_parameter(
		&self,
		vault_swap: VaultSwapDetails<AddressString>,
	) -> RpcResult<VaultSwapInputRpc> {
		Ok(vault_swap_input_encoded_to_rpc(
			self.rpc_backend
				.client
				.runtime_api()
				.cf_decode_vault_swap_parameter(
					self.rpc_backend.client.info().best_hash,
					self.signed_pool_client.account_id(),
					vault_swap.map_btc_address(Into::into),
				)
				.map_err(CfApiError::from)?
				.map_err(CfApiError::from)?,
		))
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
			.rpc_backend
			.client
			.runtime_api()
			.cf_encode_cf_parameters(
				self.rpc_backend.client.info().best_hash,
				self.signed_pool_client.account_id(),
				source_asset,
				destination_address.try_parse_to_encoded_address(destination_asset.into())?,
				destination_asset,
				try_into_refund_parameters_encoded(refund_parameters, source_asset.into())?,
				dca_parameters,
				boost_fee.unwrap_or_default(),
				broker_commission,
				affiliate_fees.unwrap_or_default(),
				channel_metadata,
			)
			.map_err(CfApiError::from)?
			.map_err(CfApiError::from)?
			.into())
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
			.await
			.map_err(CfApiError::from)?;
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

		Ok(self
			.rpc_backend
			.client
			.runtime_api()
			.cf_get_open_deposit_channels(self.rpc_backend.client.info().best_hash, account_id)
			.map_err(CfApiError::from)?)
	}

	async fn all_open_deposit_channels(
		&self,
	) -> RpcResult<Vec<(AccountId32, ChannelActionType, ChainAccounts)>> {
		Ok(self
			.rpc_backend
			.client
			.runtime_api()
			.cf_all_open_deposit_channels(self.rpc_backend.client.info().best_hash)
			.map_err(CfApiError::from)?)
	}

	async fn subscribe_transaction_screening_events(&self, pending_sink: PendingSubscriptionSink) {
		self.rpc_backend
			.new_subscription(
				NotificationBehaviour::Finalized,
				false,
				true,
				pending_sink,
				move |client, hash| {
					Ok((*client.runtime_api())
						.cf_transaction_screening_events(hash)
						.map_err(CfApiError::from)?)
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
			.await
			.map_err(CfApiError::from)?;

		Ok(extract_from_first_matching_event!(
			events,
			cf_static_runtime::swapping::events::PrivateBrokerChannelOpened,
			{ channel_id },
			channel_id
		)
		.map_err(CfApiError::from)?)
	}

	async fn close_private_btc_channel(&self) -> RpcResult<ChannelId> {
		let ExtrinsicData { events, .. } = self
			.signed_pool_client
			.submit_watch_dynamic(
				RuntimeCall::from(pallet_cf_swapping::Call::close_private_btc_channel {}),
				false,
				true,
			)
			.await
			.map_err(CfApiError::from)?;

		Ok(extract_from_first_matching_event!(
			events,
			cf_static_runtime::swapping::events::PrivateBrokerChannelClosed,
			{ channel_id },
			channel_id
		)
		.map_err(CfApiError::from)?)
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
			.await
			.map_err(CfApiError::from)?;

		Ok(extract_from_first_matching_event!(
			events,
			cf_static_runtime::swapping::events::AffiliateRegistration,
			{ affiliate_id },
			AccountId32::from(affiliate_id.0)
		)
		.map_err(CfApiError::from)?)
	}

	async fn get_affiliates(
		&self,
		affiliate: Option<AccountId32>,
	) -> RpcResult<Vec<(AccountId32, AffiliateDetails)>> {
		Ok(self
			.rpc_backend
			.client
			.runtime_api()
			.cf_affiliate_details(
				self.rpc_backend.client.info().best_hash,
				self.signed_pool_client.account_id(),
				affiliate,
			)
			.map_err(CfApiError::from)?)
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
			.await
			.map_err(CfApiError::from)?;

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
		)
		.map_err(CfApiError::from)?)
	}

	async fn vault_addresses(&self) -> RpcResult<VaultAddresses> {
		Ok(self
			.rpc_backend
			.client
			.runtime_api()
			.cf_vault_addresses(self.rpc_backend.client.info().best_hash)
			.map_err(CfApiError::from)?)
	}

	async fn set_vault_swap_minimum_broker_fee(
		&self,
		minimum_fee_bps: BasisPoints,
	) -> RpcResult<H256> {
		let ExtrinsicData { tx_hash, .. } = self
			.signed_pool_client
			.submit_watch_dynamic(
				RuntimeCall::from(pallet_cf_swapping::Call::set_vault_swap_minimum_broker_fee {
					minimum_fee_bps,
				}),
				false,
				true,
			)
			.await
			.map_err(CfApiError::from)?;

		Ok(tx_hash)
	}
}
