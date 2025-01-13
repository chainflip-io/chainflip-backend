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
use sp_block_builder::BlockBuilder;
use sp_core::crypto::AccountId32;
use sp_runtime::{
	traits::{Block as BlockT, Hash as HashT},
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
		+ sp_block_builder::BlockBuilder<B>
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
		+ sp_block_builder::BlockBuilder<B>
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

	/// Returns the AccountId of the current
	pub fn account_id(&self) -> AccountId32 {
		self.pair_signer.account_id.clone()
	}

	async fn next_nonce(&self, at_block: Hash) -> RpcResult<Nonce> {
		let mut current_nonce = self.nonce.write().await;

		match *current_nonce {
			Some(old_nonce) => {
				*current_nonce = Some(old_nonce + 1);
				Ok(old_nonce + 1)
			},
			None => {
				// If nonce is not set, reset it from account
				let account_nonce = self
					.client
					.runtime_api()
					.account_nonce(at_block, self.pair_signer.account_id.clone())?;
				*current_nonce = Some(account_nonce);
				Ok(account_nonce)
			},
		}
	}

	// async fn clear_nonce(&self) {
	// 	let mut current_nonce = self.nonce.write().await;
	// 	*current_nonce = None;
	// }

	// async fn set_nonce(&self, new_nonce: Nonce) {
	// 	let mut current_nonce = self.nonce.write().await;
	// 	*current_nonce = Some(new_nonce);
	// }

	// async fn current_nonce(&self, at_block: Hash) -> RpcResult<Nonce> {
	// 	let current_nonce = self.nonce.read().await;
	//
	// 	match *current_nonce {
	// 		Some(old_nonce) => Ok(old_nonce),
	// 		None => {
	// 			// If nonce is not set, reset it from account
	// 			let account_nonce = self
	// 				.client
	// 				.runtime_api()
	// 				.account_nonce(at_block, self.pair_signer.account_id.clone())?;
	// 			Ok(account_nonce)
	// 		},
	// 	}
	// }

	fn create_signed_extrinsic(&self, call: RuntimeCall, nonce: Nonce) -> RpcResult<B::Extrinsic> {
		let finalized_block_hash = self.client.info().finalized_hash;
		let finalized_block_number = self.client.info().finalized_number;
		let genesis_hash = self.client.info().genesis_hash;

		let runtime_version = self.client.runtime_api().version(finalized_block_hash)?;

		let (signed_extrinsic, lifetime) = self.pair_signer.new_signed_extrinsic(
			call,
			&runtime_version,
			genesis_hash,
			finalized_block_hash,
			finalized_block_number,
			SIGNED_EXTRINSIC_LIFETIME,
			nonce,
		);
		assert!(lifetime.contains(&(finalized_block_number + 1)));

		let call_data = signed_extrinsic.encode();

		Ok(Decode::decode(&mut &call_data[..]).map_err(internal_error)?)
	}

	fn get_extrinsic_details(
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

	/// Uses the `BlockBuilder` trait `apply_extrinsic` function to dry run the extrinsic
	/// This is the same function used by Polkadot System api rpc call `system_dryRun`.
	/// Meant to be used to quickly test if an extrinsic would result in a failure. Note that this
	/// always uses the current account nonce at the provided `block_hash`.
	pub fn dry_run_extrinsic(&self, call: RuntimeCall, at: Option<Hash>) -> RpcResult<()> {
		let at_block = at.unwrap_or_else(|| self.client.info().best_hash);

		// For apply_extrinsic call, always uses the current stored account nonce.
		// Using the signed_pool_client managed nonce, might result in apply_extrinsic Future error
		// when the signed_pool_client managed nonce is higher than the current account nonce
		let account_nonce = self
			.client
			.runtime_api()
			.account_nonce(at_block, self.pair_signer.account_id.clone())?;

		let extrinsic = self.create_signed_extrinsic(call, account_nonce)?;

		match self.client.runtime_api().apply_extrinsic(at_block, extrinsic)? {
			Ok(dispatch_result) => match dispatch_result {
				Ok(_) => Ok(()),
				Err(dispatch_error) => Err(CfApiError::ExtrinsicDispatchError(
					self.error_decoder.decode_dispatch_error(dispatch_error),
				)),
			},
			Err(e) => Err(e.into()),
		}
	}

	/// Signs and submits a `RuntimeCall` to the transaction pool without watching for its progress.
	/// if successful, it returns the transaction hash otherwise returns a CallError
	pub async fn submit_one(&self, call: RuntimeCall, at: Option<Hash>) -> RpcResult<Hash> {
		let at_block = at.unwrap_or_else(|| self.client.info().best_hash);

		let extrinsic = self.create_signed_extrinsic(call, self.next_nonce(at_block).await?)?;

		let tx_hash = self
			.pool
			.submit_one(at_block, TransactionSource::External, extrinsic)
			.map_err(call_error)
			.await?;

		Ok(tx_hash)
	}

	/// Signs and submits a `RuntimeCall` to the transaction pool and watches its progress.
	/// `wait_for` param determines whether to wait until the extrinsic is in a best block or a
	/// finalized block. Once the extrinsic is in a block, `ExtrinsicDetails` is returned.
	/// If an error occurs, it
	/// NB: This is a blocking call, if wait_for == InBlock it takes around 1 block (6 secs)
	/// and if wait_for == Finalized it takes around >12 secs
	pub async fn submit_watch(
		&self,
		call: RuntimeCall,
		wait_for: WaitFor,
		at: Option<Hash>,
	) -> RpcResult<ExtrinsicDetails> {
		let at_block = at.unwrap_or_else(|| self.client.info().best_hash);

		let extrinsic = self.create_signed_extrinsic(call, self.next_nonce(at_block).await?)?;

		// Validates transaction using runtime, submits to transaction pool and watches its status
		let mut status_stream = match self
			.pool
			.submit_and_watch(at_block, TransactionSource::External, extrinsic)
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
					//log::warn!("Transaction in progress status: {:?}", status);
					continue
				},
				TransactionStatus::Invalid => {
					//log::warn!("Transaction failed status: {:?}", status);
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
