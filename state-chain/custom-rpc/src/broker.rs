use crate::{pool_client::SignedPoolClient, CfApiError, RpcResult};
use anyhow::anyhow;
use cf_chains::{
	address::AddressString, CcmChannelMetadata, ChannelRefundParametersEncoded,
	ChannelRefundParametersGeneric,
};
use cf_primitives::{
	AffiliateShortId, Affiliates, Asset, BasisPoints, BlockNumber, ChannelId, DcaParameters,
};
use cf_rpc_types::{
	broker::{
		find_lowest_unused_short_id, GetOpenDepositChannelsQuery, RefundParameters,
		SwapDepositAddress, TransactionInId, WithdrawFeesDetail,
	},
	extract_event,
};
use cf_utilities::{rpc::NumberOrHex, try_parse_number_or_hex};
use jsonrpsee::{core::async_trait, proc_macros::rpc};
use sc_client_api::{
	blockchain::HeaderMetadata, Backend, BlockBackend, HeaderBackend, StorageProvider,
};
use sp_api::CallApiAt;
use sp_core::crypto::AccountId32;
use sp_runtime::traits::Block as BlockT;
use state_chain_runtime::{
	runtime_apis::{ChainAccounts, CustomRuntimeApi, VaultSwapDetails},
	AccountId, Nonce, RuntimeCall,
};
use std::sync::Arc;

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
		refund_parameters: Option<RefundParameters>,
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
		min_output_amount: NumberOrHex,
		retry_duration: BlockNumber,
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

	#[method(name = "open_private_btc_channel", aliases = ["broker_openPrivateBtcChannel"])]
	async fn open_private_btc_channel(&self) -> RpcResult<ChannelId>;

	#[method(name = "close_private_btc_channel", aliases = ["broker_closePrivateBtcChannel"])]
	async fn close_private_btc_channel(&self) -> RpcResult<ChannelId>;

	#[method(name = "register_affiliate", aliases = ["broker_registerAffiliate"])]
	async fn register_affiliate(
		&self,
		affiliate_id: AccountId32,
		short_id: Option<AffiliateShortId>,
	) -> RpcResult<AffiliateShortId>;

	#[method(name = "get_affiliates", aliases = ["broker_getAffiliates"])]
	async fn get_affiliates(&self) -> RpcResult<Vec<(AffiliateShortId, AccountId32)>>;
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
	pub client: Arc<C>,
	pub signed_pool_client: SignedPoolClient<C, B, BE>,
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
		let (tx_hash, _, _, _) = self
			.signed_pool_client
			.submit_watch(
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
		refund_parameters: Option<RefundParameters>,
		dca_parameters: Option<DcaParameters>,
	) -> RpcResult<SwapDepositAddress> {
		let destination_address = destination_address
			.try_parse_to_encoded_address(destination_asset.into())
			.map_err(anyhow::Error::msg)?;

		let (_, events, header, _) = self
			.signed_pool_client
			.submit_watch(
				RuntimeCall::from(
					pallet_cf_swapping::Call::request_swap_deposit_address_with_affiliates {
						source_asset,
						destination_asset,
						destination_address,
						broker_commission,
						channel_metadata,
						boost_fee: boost_fee.unwrap_or_default(),
						affiliate_fees: affiliate_fees.unwrap_or_default(),
						refund_parameters: refund_parameters
							.map(|rpc_params: ChannelRefundParametersGeneric<AddressString>| {
								Ok::<_, anyhow::Error>(ChannelRefundParametersEncoded {
									retry_duration: rpc_params.retry_duration,
									refund_address: rpc_params
										.refund_address
										.try_parse_to_encoded_address(source_asset.into())?,
									min_price: rpc_params.min_price,
								})
							})
							.transpose()?,
						dca_parameters,
					},
				),
				false,
				true,
			)
			.await?;

		Ok(extract_event!(
			events,
			state_chain_runtime::RuntimeEvent::Swapping,
			pallet_cf_swapping::Event::SwapDepositAddressReady,
			{
				deposit_address,
				channel_id,
				source_chain_expiry_block,
				channel_opening_fee,
				refund_parameters,
				..
			},
			SwapDepositAddress {
				address: AddressString::from_encoded_address(deposit_address),
				issued_block: header.number,
				channel_id: *channel_id,
				source_chain_expiry_block: (*source_chain_expiry_block).into(),
				channel_opening_fee: (*channel_opening_fee).into(),
				refund_parameters: refund_parameters.as_ref().map(|params| {
					params.map_address(|refund_address| {
						AddressString::from_encoded_address(&refund_address)
					})
				}),
			}
		)?)
	}

	async fn withdraw_fees(
		&self,
		asset: Asset,
		destination_address: AddressString,
	) -> RpcResult<WithdrawFeesDetail> {
		let (tx_hash, events, _, _) = self
			.signed_pool_client
			.submit_watch(
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

		Ok(extract_event!(
			events,
			state_chain_runtime::RuntimeEvent::Swapping,
			pallet_cf_swapping::Event::WithdrawalRequested,
			{
				egress_amount,
				egress_fee,
				destination_address,
				egress_id,
				..
			},
			WithdrawFeesDetail {
				tx_hash,
				egress_id: *egress_id,
				egress_amount: (*egress_amount).into(),
				egress_fee: (*egress_fee).into(),
				destination_address: AddressString::from_encoded_address(destination_address),
			}
		)?)
	}

	// This is also defined in custom-rpc as cf_get_vault_swap_details. This is required here
	// to make a smooth migration from broker API binary. TODO: consider defining only in 1 place
	async fn request_swap_parameter_encoding(
		&self,
		source_asset: Asset,
		destination_asset: Asset,
		destination_address: AddressString,
		broker_commission: BasisPoints,
		min_output_amount: NumberOrHex,
		retry_duration: BlockNumber,
		boost_fee: Option<BasisPoints>,
		affiliate_fees: Option<Affiliates<AccountId32>>,
		dca_parameters: Option<DcaParameters>,
	) -> RpcResult<VaultSwapDetails<AddressString>> {
		Ok(self
			.client
			.runtime_api()
			.cf_get_vault_swap_details(
				self.client.info().best_hash,
				self.signed_pool_client.account_id(),
				source_asset,
				destination_asset,
				destination_address.try_parse_to_encoded_address(destination_asset.into())?,
				broker_commission,
				try_parse_number_or_hex(min_output_amount)?,
				retry_duration,
				boost_fee.unwrap_or_default(),
				affiliate_fees.unwrap_or_default(),
				dca_parameters,
			)??
			.map_btc_address(Into::into))
	}

	async fn mark_transaction_for_rejection(&self, tx_id: TransactionInId) -> RpcResult<()> {
		match tx_id {
			TransactionInId::Bitcoin(tx_id) =>
				self.signed_pool_client
					.submit_watch(
						RuntimeCall::BitcoinIngressEgress(
							pallet_cf_ingress_egress::Call::mark_transaction_for_rejection {
								tx_id,
							},
						),
						false,
						true,
					)
					.await,
		}?;
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

		self.client
			.runtime_api()
			.cf_get_open_deposit_channels(self.client.info().best_hash, account_id)
			.map_err(CfApiError::RuntimeApiError)
	}

	async fn open_private_btc_channel(&self) -> RpcResult<ChannelId> {
		let (_, events, _, _) = self
			.signed_pool_client
			.submit_watch(
				RuntimeCall::from(pallet_cf_swapping::Call::open_private_btc_channel {}),
				false,
				true,
			)
			.await?;

		Ok(extract_event!(
			events,
			state_chain_runtime::RuntimeEvent::Swapping,
			pallet_cf_swapping::Event::PrivateBrokerChannelOpened,
			{ channel_id, .. },
			*channel_id
		)?)
	}

	async fn close_private_btc_channel(&self) -> RpcResult<ChannelId> {
		let (_, events, _, _) = self
			.signed_pool_client
			.submit_watch(
				RuntimeCall::from(pallet_cf_swapping::Call::close_private_btc_channel {}),
				false,
				true,
			)
			.await?;

		Ok(extract_event!(
			events,
			state_chain_runtime::RuntimeEvent::Swapping,
			pallet_cf_swapping::Event::PrivateBrokerChannelClosed,
			{ channel_id, .. },
			*channel_id
		)?)
	}

	async fn register_affiliate(
		&self,
		affiliate_id: AccountId32,
		short_id: Option<AffiliateShortId>,
	) -> RpcResult<AffiliateShortId> {
		let register_as_id = if let Some(short_id) = short_id {
			short_id
		} else {
			let affiliates = self.get_affiliates().await?;

			// Check if the affiliate is already registered
			if let Some((existing_short_id, _)) =
				affiliates.iter().find(|(_, id)| id == &affiliate_id)
			{
				return Ok(*existing_short_id);
			}

			// Auto assign the lowest unused short id
			let used_ids: Vec<AffiliateShortId> =
				affiliates.into_iter().map(|(short_id, _)| short_id).collect();
			find_lowest_unused_short_id(&used_ids)?
		};

		let (_, events, _, _) = self
			.signed_pool_client
			.submit_watch(
				RuntimeCall::from(pallet_cf_swapping::Call::register_affiliate {
					affiliate_id,
					short_id: register_as_id,
				}),
				false,
				true,
			)
			.await?;

		Ok(extract_event!(
			events,
			state_chain_runtime::RuntimeEvent::Swapping,
			pallet_cf_swapping::Event::AffiliateRegistrationUpdated,
			{ affiliate_short_id, .. },
			*affiliate_short_id
		)?)
	}

	async fn get_affiliates(&self) -> RpcResult<Vec<(AffiliateShortId, AccountId32)>> {
		self.client
			.runtime_api()
			.cf_get_affiliates(self.client.info().best_hash, self.signed_pool_client.account_id())
			.map_err(CfApiError::RuntimeApiError)
	}
}
