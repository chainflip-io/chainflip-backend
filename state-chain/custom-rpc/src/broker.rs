use crate::{
	call_error,
	crypto::{PairSigner, SubxtSignerInterface},
	internal_error,
	subxt_state_chain_config::StateChainConfig,
	CfApiError, ExtrinsicDispatchError, RpcResult, StorageQueryApi,
};
use anyhow::anyhow;
use cf_chains::{
	address::AddressString, CcmChannelMetadata, ChannelRefundParametersEncoded,
	ChannelRefundParametersGeneric,
};
use cf_primitives::{
	AffiliateShortId, Affiliates, Asset, BasisPoints, BlockNumber, ChannelId, DcaParameters,
};
use cf_utilities::{rpc::NumberOrHex, try_parse_number_or_hex};
use chainflip_integrator::{
	extract_event, find_lowest_unused_short_id, GetOpenDepositChannelsQuery, RefundParameters,
	SwapDepositAddress, TransactionInId, WithdrawFeesDetail,
};
use codec::{Decode, Encode};
use frame_support::{dispatch::DispatchInfo, Deserialize, Serialize};
use frame_system_rpc_runtime_api::AccountNonceApi;
use futures::{StreamExt, TryFutureExt};
use jsonrpsee::{core::async_trait, proc_macros::rpc};
use sc_client_api::{
	blockchain::HeaderMetadata, Backend, BlockBackend, HeaderBackend, StorageProvider,
};
use sc_transaction_pool::{ChainApi, FullPool};
use sc_transaction_pool_api::{TransactionPool, TransactionStatus};
use sp_api::{CallApiAt, Core};
use sp_core::crypto::AccountId32;
use sp_runtime::{
	traits::{Block as BlockT, Hash as HashT, Header},
	transaction_validity::TransactionSource,
};
use state_chain_runtime::{
	constants::common::SIGNED_EXTRINSIC_LIFETIME,
	runtime_apis::{ChainAccounts, CustomRuntimeApi, VaultSwapDetails},
	AccountId, Hash, Nonce, RuntimeCall,
};
use std::{marker::PhantomData, sync::Arc};
use subxt::{
	config::DefaultExtrinsicParamsBuilder, ext::frame_metadata, tx::DynamicPayload, OfflineClient,
	OnlineClient,
};

#[rpc(server, client, namespace = "broker")]
pub trait BrokerSignedApi {
	#[method(name = "send_remark")]
	async fn cf_send_remark(&self) -> RpcResult<()>;

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

/// An Broker signed RPC extension for the state chain node.
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
		+ Core<B>
		+ sp_transaction_pool::runtime_api::TaggedTransactionQueue<B>
		+ frame_system_rpc_runtime_api::AccountNonceApi<B, AccountId, Nonce>,
{
	pub client: Arc<C>,
	pub backend: Arc<BE>,
	pub pool: Arc<FullPool<B, C>>,
	pub executor: Arc<dyn sp_core::traits::SpawnNamed>,
	pub _phantom: PhantomData<B>,
	pub signer: SubxtSignerInterface<sp_core::sr25519::Pair>,
	pub pair_signer: PairSigner<sp_core::sr25519::Pair>,
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
		+ Core<B>
		+ sp_transaction_pool::runtime_api::TaggedTransactionQueue<B>
		+ frame_system_rpc_runtime_api::AccountNonceApi<B, AccountId, Nonce>,
{
	pub fn with_offline_subxt(&self, hash: Hash) -> RpcResult<OfflineClient<StateChainConfig>> {
		let genesis_hash =
			self.client.block_hash(0).ok().flatten().expect("Genesis block exists; qed");
		let version = self.client.runtime_api().version(hash)?;

		let metadata = frame_metadata::RuntimeMetadataPrefixed::decode(
			&mut state_chain_runtime::Runtime::metadata_at_version(15)
				.expect("Version 15 should be supported by the runtime.")
				.as_slice(),
		)
		.expect("Runtime metadata should be valid.");

		Ok(OfflineClient::<StateChainConfig>::new(
			genesis_hash,
			subxt::client::RuntimeVersion {
				spec_version: version.spec_version,
				transaction_version: version.transaction_version,
			},
			subxt::Metadata::try_from(metadata).map_err(internal_error)?,
		))
	}

	pub async fn with_online_subxt(&self) -> RpcResult<OnlineClient<StateChainConfig>> {
		Ok(OnlineClient::<StateChainConfig>::new().await.map_err(internal_error)?)
	}

	pub fn create_dynamic_signed_extrinsic(
		&self,
		block_hash: Hash,
		tx_payload: DynamicPayload,
	) -> RpcResult<B::Extrinsic> {
		let Some(header) = self.client.header(block_hash)? else {
			Err(internal_error(format!("Could not fetch block header for block {:?}", block_hash)))?
		};

		let account_nonce =
			self.client.runtime_api().account_nonce(block_hash, self.signer.account())?;

		let subxt = self.with_offline_subxt(block_hash)?;

		let tx_params = DefaultExtrinsicParamsBuilder::<StateChainConfig>::new()
			.mortal_unchecked(header.number.into(), block_hash, SIGNED_EXTRINSIC_LIFETIME.into())
			.nonce(account_nonce.into())
			.build();

		let call_data = subxt
			.tx()
			.create_signed_offline(&tx_payload, &self.signer, tx_params)
			.map_err(internal_error)?
			.into_encoded();

		Ok(Decode::decode(&mut &call_data[..]).map_err(internal_error)?)
	}

	pub fn create_signed_extrinsic(
		&self,
		current_hash: Hash,
		call: RuntimeCall,
	) -> RpcResult<B::Extrinsic> {
		let Some(current_header) = self.client.header(current_hash)? else {
			Err(internal_error(format!(
				"Could not fetch block header for block {:?}",
				current_hash
			)))?
		};
		let Some(genesis_hash) = self.client.block_hash(0).ok().flatten() else {
			Err(internal_error("Could not fetch genesis hash".to_string()))?
		};

		let runtime_version = self.client.runtime_api().version(current_hash)?;

		let account_nonce =
			self.client.runtime_api().account_nonce(current_hash, self.signer.account())?;

		let (signed_extrinsic, _) = self.pair_signer.new_signed_extrinsic(
			call,
			&runtime_version,
			genesis_hash,
			current_hash,
			*current_header.number(),
			SIGNED_EXTRINSIC_LIFETIME,
			account_nonce,
		);

		let call_data = signed_extrinsic.encode();

		Ok(Decode::decode(&mut &call_data[..]).map_err(internal_error)?)
	}

	pub fn decode_extrinsic_dispatch_error(
		&self,
		metadata: &subxt::Metadata,
		dispatch_error: sp_runtime::DispatchError,
	) -> ExtrinsicDispatchError {
		match dispatch_error {
			sp_runtime::DispatchError::Module(module_error) => {
				if let Some((pallet, error_variant)) =
					metadata.pallet_by_index(module_error.index).and_then(|pallet_metadata| {
						u8::decode(&mut &module_error.error[..]).ok().and_then(|error_index| {
							pallet_metadata
								.error_variant_by_index(error_index)
								.map(|variant| (pallet_metadata.name().to_string(), variant))
						})
					}) {
					ExtrinsicDispatchError::KnownModuleError {
						pallet,
						name: error_variant.name.to_string(),
						error: error_variant.docs.join(" "),
					}
				} else {
					ExtrinsicDispatchError::OtherDispatchError(sp_runtime::DispatchError::Module(
						module_error,
					))
				}
			},
			dispatch_error => ExtrinsicDispatchError::OtherDispatchError(dispatch_error),
		}
	}

	pub fn get_extrinsic_details(
		&self,
		block_hash: Hash,
		extrinsic_index: usize,
	) -> RpcResult<ExtrinsicDetails> {
		let metadata = self.with_offline_subxt(block_hash)?.metadata();

		let Some(signed_block) = self.client.block(block_hash)? else {
			Err(CfApiError::OtherError(anyhow!("The signed block this transaction was not found")))?
		};
		let Some(extrinsic) = signed_block.block.extrinsics().get(extrinsic_index) else {
			Err(CfApiError::OtherError(anyhow!(
				"The signed block extrinsics does not have an extrinsic at index {:?}",
				extrinsic_index
			)))?
		};

		let block_events = StorageQueryApi::new(&self.client)
			.get_storage_value::<frame_system::Events<state_chain_runtime::Runtime>, _>(
				block_hash,
			)?;

		let extrinsic_events = block_events
			.iter()
			.filter_map(|event_record| match event_record.as_ref() {
				frame_system::EventRecord {
					phase: frame_system::Phase::ApplyExtrinsic(index),
					event,
					..
				} if *index as usize == extrinsic_index => Some(event.clone()),
				_ => None,
			})
			.collect::<Vec<_>>();

		let tx_hash =
			<state_chain_runtime::Runtime as frame_system::Config>::Hashing::hash_of(extrinsic);

		// We expect to find a Success or Failed event, grab the dispatch info and send
		// it with the events
		extrinsic_events
			.iter()
			.find_map(|event| match event {
				state_chain_runtime::RuntimeEvent::System(
					frame_system::Event::ExtrinsicSuccess { dispatch_info },
				) => Some(Ok(*dispatch_info)),
				state_chain_runtime::RuntimeEvent::System(
					frame_system::Event::ExtrinsicFailed { dispatch_error, dispatch_info: _ },
				) => Some(Err(CfApiError::ExtrinsicDispatchError(
					self.decode_extrinsic_dispatch_error(&metadata, *dispatch_error),
				))),
				_ => None,
			})
			.expect("Unexpected state chain node behaviour")
			.map(|dispatch_info| ExtrinsicDetails {
				tx_hash,
				header: signed_block.block.header().clone(),
				events: extrinsic_events,
				_dispatch_info: dispatch_info,
			})
	}

	pub async fn submit_one(&self, call: RuntimeCall, at: Option<Hash>) -> RpcResult<Hash> {
		let block_hash = at.unwrap_or_else(|| self.client.info().best_hash);

		let extrinsic = self.create_signed_extrinsic(block_hash, call)?;

		let tx_hash = self
			.pool
			.submit_one(block_hash, TransactionSource::External, extrinsic)
			.map_err(call_error)
			.await?;

		Ok(tx_hash)
	}

	pub async fn submit_watch(
		&self,
		call: RuntimeCall,
		wait_for: WaitFor,
		at: Option<Hash>,
	) -> RpcResult<ExtrinsicDetails> {
		let block_hash = at.unwrap_or_else(|| self.client.info().best_hash);

		let extrinsic = self.create_signed_extrinsic(block_hash, call)?;

		// validate transaction
		self.pool
			.api()
			.validate_transaction(block_hash, TransactionSource::External, extrinsic.clone())
			.await??;

		let mut status_stream = self
			.pool
			.submit_and_watch(block_hash, TransactionSource::External, extrinsic)
			.await?;

		// Periodically poll the transaction pool to check inclusion status
		while let Some(status) = status_stream.next().await {
			log::warn!(" ------ transaction status: {:?}", status);

			match status {
				TransactionStatus::InBlock((block_hash, tx_index)) => {
					if matches!(wait_for, WaitFor::InBlock) {
						return self.get_extrinsic_details(block_hash, tx_index);
					}
				},
				TransactionStatus::Finalized((block_hash, tx_index)) => {
					if matches!(wait_for, WaitFor::Finalized) {
						return self.get_extrinsic_details(block_hash, tx_index);
					}
				},
				TransactionStatus::Future |
				TransactionStatus::Ready |
				TransactionStatus::Broadcast(_) => {
					log::warn!("Transaction in progress status: {:?}", status);
					continue
				},
				TransactionStatus::Invalid => {
					log::warn!("Transaction failed status: {:?}", status);
					return Err(CfApiError::OtherError(anyhow!(
						"transaction is no longer valid in the current state. "
					)))
				},
				TransactionStatus::Dropped => {
					log::warn!("Transaction failed status: {:?}", status);
					return Err(CfApiError::OtherError(anyhow!(
						"transaction was dropped from the pool because of the limit"
					)))
				},
				TransactionStatus::Usurped(_hash) => {
					log::warn!("Transaction failed status: {:?}", status);
					return Err(CfApiError::OtherError(anyhow!(
						"Transaction has been replaced in the pool, "
					)))
				},
				TransactionStatus::FinalityTimeout(_block_hash) => {
					log::warn!("Transaction failed status: {:?}", status);
					//return Err(CfApiError::OtherError(anyhow!("Maximum number of finality
					// watchers has been reached")))
					continue
				},
				TransactionStatus::Retracted(_block_hash) => {
					log::warn!("Transaction failed status: {:?}", status);
					return Err(CfApiError::OtherError(anyhow!(
						"The block this transaction was included in has been retracted."
					)))
				},
			}
		}

		Err(CfApiError::OtherError(anyhow!("transaction unexpected error")))
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
		+ CallApiAt<B>
		+ StorageProvider<B, BE>
		+ sp_api::ProvideRuntimeApi<B>
		+ sp_runtime::traits::BlockIdTo<B>,
	C::Api: CustomRuntimeApi<B>
		+ Core<B>
		+ sp_transaction_pool::runtime_api::TaggedTransactionQueue<B>
		+ frame_system_rpc_runtime_api::AccountNonceApi<B, AccountId, Nonce>,
{
	// async fn cf_send_remark(&self) -> RpcResult<()> {
	// 	let subxt = self.with_online_subxt().await?;
	//
	// 	let tx_payload = subxt::dynamic::tx(
	// 		"System",
	// 		"remark",
	// 		vec![subxt::dynamic::Value::from_bytes("Hello from Chainflip RPC 2.0")],
	// 	);
	//
	// 	let _events = subxt
	// 		.tx()
	// 		.sign_and_submit_then_watch_default(&tx_payload, &self.signer)
	// 		.await
	// 		.map_err(internal_error)?;
	//
	// 	Ok(())
	// }

	async fn cf_send_remark(&self) -> RpcResult<()> {
		let best_hash = self.client.info().best_hash;

		let extrinsic = self.create_dynamic_signed_extrinsic(
			best_hash,
			subxt::dynamic::tx(
				"System",
				"remark",
				vec![subxt::dynamic::Value::from_bytes("Hello from Chainflip RPC 2.0")],
			),
		)?;

		// validate transaction
		let _result = self
			.pool
			.api()
			.validate_transaction(best_hash, TransactionSource::External, extrinsic.clone())
			.await;

		// submit transaction
		self.pool
			.submit_one(best_hash, TransactionSource::External, extrinsic)
			.map_err(call_error)
			.await?;

		Ok(())
	}

	async fn register_account(&self) -> RpcResult<String> {
		let details = self
			.submit_watch(
				RuntimeCall::from(pallet_cf_swapping::Call::register_as_broker {}),
				WaitFor::InBlock,
				None,
			)
			.await?;

		Ok(format!("{:#x}", details.tx_hash))
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

		let extrinsic_details = self
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
				WaitFor::InBlock,
				None,
			)
			.await?;

		Ok(extract_event!(
			extrinsic_details.events,
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
				issued_block: extrinsic_details.header.number,
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
		let extrinsic_details = self
			.submit_watch(
				RuntimeCall::from(pallet_cf_swapping::Call::withdraw {
					asset,
					destination_address: destination_address
						.try_parse_to_encoded_address(asset.into())
						.map_err(anyhow::Error::msg)?,
				}),
				WaitFor::InBlock,
				None,
			)
			.await?;

		Ok(extract_event!(
			extrinsic_details.events,
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
				tx_hash: extrinsic_details.tx_hash,
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
				self.pair_signer.account_id.clone(),
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
				self.submit_watch(
					RuntimeCall::BitcoinIngressEgress(
						pallet_cf_ingress_egress::Call::mark_transaction_for_rejection { tx_id },
					),
					WaitFor::InBlock,
					None,
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
			GetOpenDepositChannelsQuery::Mine => Some(self.pair_signer.account_id.clone()),
		};

		self.client
			.runtime_api()
			.cf_get_open_deposit_channels(self.client.info().best_hash, account_id)
			.map_err(CfApiError::RuntimeApiError)
	}

	async fn open_private_btc_channel(&self) -> RpcResult<ChannelId> {
		let extrinsic_details = self
			.submit_watch(
				RuntimeCall::from(pallet_cf_swapping::Call::open_private_btc_channel {}),
				WaitFor::InBlock,
				None,
			)
			.await?;

		Ok(extract_event!(
			&extrinsic_details.events,
			state_chain_runtime::RuntimeEvent::Swapping,
			pallet_cf_swapping::Event::PrivateBrokerChannelOpened,
			{ channel_id, .. },
			*channel_id
		)?)
	}

	async fn close_private_btc_channel(&self) -> RpcResult<ChannelId> {
		let extrinsic_details = self
			.submit_watch(
				RuntimeCall::from(pallet_cf_swapping::Call::close_private_btc_channel {}),
				WaitFor::InBlock,
				None,
			)
			.await?;

		Ok(extract_event!(
			&extrinsic_details.events,
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

		let extrinsic_details = self
			.submit_watch(
				RuntimeCall::from(pallet_cf_swapping::Call::register_affiliate {
					affiliate_id,
					short_id: register_as_id,
				}),
				WaitFor::InBlock,
				None,
			)
			.await?;

		Ok(extract_event!(
			&extrinsic_details.events,
			state_chain_runtime::RuntimeEvent::Swapping,
			pallet_cf_swapping::Event::AffiliateRegistrationUpdated,
			{ affiliate_short_id, .. },
			*affiliate_short_id
		)?)
	}

	async fn get_affiliates(&self) -> RpcResult<Vec<(AffiliateShortId, AccountId32)>> {
		self.client
			.runtime_api()
			.cf_get_affiliates(self.client.info().best_hash, self.pair_signer.account_id.clone())
			.map_err(CfApiError::RuntimeApiError)
	}
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
pub enum WaitFor {
	// Wait until the extrinsic is included in a block
	InBlock,
	// Wait until the extrinsic is in a finalized block
	#[default]
	Finalized,
}

pub struct ExtrinsicDetails {
	tx_hash: Hash,
	header: state_chain_runtime::Header,
	events: Vec<state_chain_runtime::RuntimeEvent>,
	_dispatch_info: DispatchInfo,
}
