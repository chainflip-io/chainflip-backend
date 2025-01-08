use crate::{call_error, internal_error, CfApiError, RpcResult, StorageQueryApi};
use anyhow::anyhow;
use chainflip_integrator::{error_decoder::ErrorDecoder, signer::PairSigner};
use codec::{Decode, Encode};
use frame_support::{dispatch::DispatchInfo, Deserialize, Serialize};
use frame_system_rpc_runtime_api::AccountNonceApi;
use futures::{StreamExt, TryFutureExt};
use jsonrpsee::tokio::sync::RwLock;
use sc_client_api::{
	blockchain::HeaderMetadata, Backend, BlockBackend, HeaderBackend, StorageProvider,
};
use sc_transaction_pool::FullPool;
use sc_transaction_pool_api::{TransactionPool, TransactionStatus};
use sp_api::{CallApiAt, Core};
use sp_core::crypto::AccountId32;
use sp_runtime::{
	traits::{Block as BlockT, Hash as HashT, Header},
	transaction_validity::TransactionSource,
};
use state_chain_runtime::{
	constants::common::SIGNED_EXTRINSIC_LIFETIME, runtime_apis::CustomRuntimeApi, AccountId, Hash,
	Nonce, RuntimeCall,
};
use std::{marker::PhantomData, sync::Arc};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
pub enum WaitFor {
	// Wait until the extrinsic is included in a block
	InBlock,
	// Wait until the extrinsic is in a finalized block
	#[default]
	Finalized,
}

pub struct ExtrinsicDetails {
	pub tx_hash: Hash,
	pub header: state_chain_runtime::Header,
	pub events: Vec<state_chain_runtime::RuntimeEvent>,
	pub _dispatch_info: DispatchInfo,
}

pub struct SignedPoolClient<C, B, BE>
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
	pub pool: Arc<FullPool<B, C>>,
	pub executor: Arc<dyn sp_core::traits::SpawnNamed>,
	pub _phantom: PhantomData<B>,
	pub _phantom_b: PhantomData<BE>,
	pub pair_signer: PairSigner<sp_core::sr25519::Pair>,
	pub error_decoder: ErrorDecoder,
	pub nonce: Arc<RwLock<Option<Nonce>>>,
}

impl<C, B, BE> SignedPoolClient<C, B, BE>
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
	pub fn new(
		client: Arc<C>,
		pool: Arc<FullPool<B, C>>,
		executor: Arc<dyn sp_core::traits::SpawnNamed>,
		pair: sp_core::sr25519::Pair,
	) -> Self {
		Self {
			client,
			pool,
			executor,
			_phantom: Default::default(),
			_phantom_b: Default::default(),
			pair_signer: PairSigner::new(pair),
			error_decoder: ErrorDecoder::default(),
			nonce: Arc::new(RwLock::new(None)),
		}
	}

	pub fn account_id(&self) -> AccountId32 {
		self.pair_signer.account_id.clone()
	}

	// async fn clear_nonce(&self) {
	// 	let mut current_nonce = self.nonce.write().await;
	// 	*current_nonce = None;
	// }

	async fn set_nonce(&self, new_nonce: Nonce) {
		let mut current_nonce = self.nonce.write().await;
		*current_nonce = Some(new_nonce);
	}

	async fn current_nonce(&self, at_block: Hash) -> RpcResult<Nonce> {
		let current_nonce = self.nonce.read().await;

		match *current_nonce {
			Some(old_nonce) => Ok(old_nonce),
			None => {
				// If nonce is not set, reset it from account
				let account_nonce = self
					.client
					.runtime_api()
					.account_nonce(at_block, self.pair_signer.account_id.clone())?;
				Ok(account_nonce)
			},
		}
	}

	pub fn create_signed_extrinsic(
		&self,
		current_hash: Hash,
		call: RuntimeCall,
		nonce: Nonce,
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

		let (signed_extrinsic, _) = self.pair_signer.new_signed_extrinsic(
			call,
			&runtime_version,
			genesis_hash,
			current_hash,
			*current_header.number(),
			SIGNED_EXTRINSIC_LIFETIME,
			nonce,
		);

		let call_data = signed_extrinsic.encode();

		Ok(Decode::decode(&mut &call_data[..]).map_err(internal_error)?)
	}

	pub fn get_extrinsic_details(
		&self,
		block_hash: Hash,
		extrinsic_index: usize,
	) -> RpcResult<ExtrinsicDetails> {
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
					self.error_decoder.decode_dispatch_error(*dispatch_error),
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

		let current_nonce = self.current_nonce(block_hash).await?;
		let extrinsic = self.create_signed_extrinsic(block_hash, call, current_nonce)?;

		// validate transaction
		// self.pool
		// 	.api()
		// 	.validate_transaction(block_hash, TransactionSource::External, extrinsic.clone())
		// 	.await??;

		let tx_hash = self
			.pool
			.submit_one(block_hash, TransactionSource::External, extrinsic)
			.map_err(call_error)
			.await?;

		// Increment nonce for next transaction
		self.set_nonce(current_nonce + 1).await;

		Ok(tx_hash)
	}

	pub async fn submit_watch(
		&self,
		call: RuntimeCall,
		wait_for: WaitFor,
		at: Option<Hash>,
	) -> RpcResult<ExtrinsicDetails> {
		let block_hash = at.unwrap_or_else(|| self.client.info().best_hash);

		let current_nonce = self.current_nonce(block_hash).await?;
		// Increment nonce for next transaction
		self.set_nonce(current_nonce + 1).await;
		let extrinsic = self.create_signed_extrinsic(block_hash, call, current_nonce + 1)?;

		// validate transaction
		// let val = self.pool
		// 	.api()
		// 	.validate_transaction(block_hash, TransactionSource::External, extrinsic.clone())
		// 	.await??;

		let mut status_stream = match self
			.pool
			.submit_and_watch(block_hash, TransactionSource::External, extrinsic)
			.await
		{
			Ok(stream) => stream,
			Err(e) => {
				log::error!(" ------ submit_and_watch error: {:?}", e);
				return Err(e.into());
			},
		};

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
