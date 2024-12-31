use crate::{
	call_error, crypto::SubxtSignerInterface, internal_error,
	subxt_state_chain_config::StateChainConfig, CfApiError, ExtrinsicDispatchError, RpcResult,
	StorageQueryApi,
};
use anyhow::anyhow;
use cf_chains::address::AddressString;
use cf_primitives::{Affiliates, Asset, BasisPoints, BlockNumber, DcaParameters};
use cf_utilities::{rpc::NumberOrHex, try_parse_number_or_hex};
use codec::Decode;
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
	traits::{Block as BlockT, Hash as HashT},
	transaction_validity::TransactionSource,
};
use state_chain_runtime::{
	constants::common::SIGNED_EXTRINSIC_LIFETIME,
	runtime_apis::{CustomRuntimeApi, VaultSwapDetails},
	AccountId, Hash, Nonce,
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

	pub fn create_signed_extrinsic(
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
				_header: signed_block.block.header().clone(),
				_events: extrinsic_events,
				_dispatch_info: dispatch_info,
			})
	}

	pub async fn submit_one(&self, block_hash: Hash, extrinsic: B::Extrinsic) -> RpcResult<Hash> {
		let tx_hash = self
			.pool
			.submit_one(block_hash, TransactionSource::External, extrinsic)
			.map_err(call_error)
			.await?;

		Ok(tx_hash)
	}

	pub async fn submit_watch(
		&self,
		block_hash: Hash,
		extrinsic: B::Extrinsic,
		wait_for: WaitFor,
	) -> RpcResult<ExtrinsicDetails> {
		match wait_for {
			WaitFor::InBlock | WaitFor::Finalized => {
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
			},
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

		let extrinsic = self.create_signed_extrinsic(
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
		let best_hash = self.client.info().best_hash;

		let extrinsic = self.create_signed_extrinsic(
			best_hash,
			subxt::dynamic::tx(
				"Swapping",
				"register_as_broker",
				Vec::<subxt::dynamic::Value>::with_capacity(0),
			),
		)?;

		// validate transaction
		// match self
		// 	.pool
		// 	.api()
		// 	.validate_transaction(best_hash, TransactionSource::External, extrinsic.clone())
		// 	.await {
		//
		// 	Ok(_) => {
		// 		let result = self.submit_watch(best_hash, extrinsic, WaitFor::Finalized).await?;
		//
		// 		log::warn!("result is '{:?}'", result);
		// 		let WaitForResult::BlockHash(tx_hash) = result else {
		// 			Err(internal_error("invalid block hash"))?
		// 		};
		// 		Ok(format!("{tx_hash:#x}"))
		// 	}
		// 	Err(e) => {
		// 		Err(e.into())
		// 	}
		// }

		let details = self.submit_watch(best_hash, extrinsic, WaitFor::InBlock).await?;

		Ok(format!("{:#x}", details.tx_hash))
	}

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
				self.signer.account(),
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
	_header: state_chain_runtime::Header,
	_events: Vec<state_chain_runtime::RuntimeEvent>,
	_dispatch_info: DispatchInfo,
}
