use crate::{internal_error, CfApiError, RpcResult, StorageQueryApi};
use anyhow::anyhow;
use cf_node_clients::{
	runtime_decoder::{build_runtime_version, RuntimeDecoder},
	signer::PairSigner,
	ExtrinsicData, ExtrinsicDetails, WaitFor, WaitForDynamicResult, WaitForResult,
};
use codec::{Decode, Encode};
use frame_system_rpc_runtime_api::AccountNonceApi;
use futures::StreamExt;
use jsonrpsee::tokio::sync::{RwLock, RwLockReadGuard};
use sc_client_api::{
	blockchain::HeaderMetadata, Backend, BlockBackend, HeaderBackend, StorageProvider,
};
use sc_transaction_pool::FullPool;
use sc_transaction_pool_api::{TransactionPool, TransactionStatus};
use sp_api::{CallApiAt, Core, Metadata};
use sp_block_builder::BlockBuilder;
use sp_core::crypto::AccountId32;
use sp_runtime::{
	traits::{Block as BlockT, Hash as HashT},
	transaction_validity::TransactionSource,
	Either,
};
use state_chain_runtime::{
	constants::common::SIGNED_EXTRINSIC_LIFETIME, runtime_apis::CustomRuntimeApi, AccountId, Hash,
	Nonce, RuntimeCall,
};
use std::{collections::HashMap, future::Future, marker::PhantomData, ops::Deref, sync::Arc};

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
		+ sp_api::Core<B>
		+ sp_api::Metadata<B>
		+ sp_block_builder::BlockBuilder<B>
		+ sp_transaction_pool::runtime_api::TaggedTransactionQueue<B>
		+ frame_system_rpc_runtime_api::AccountNonceApi<B, AccountId, Nonce>,
{
	pub client: Arc<C>,
	pub pool: Arc<FullPool<B, C>>,
	pub _phantom_b: PhantomData<B>,
	pub _phantom_be: PhantomData<BE>,
	pub pair_signer: PairSigner<sp_core::sr25519::Pair>,
	pub runtime_decoders: Arc<RwLock<HashMap<u32, RuntimeDecoder>>>,
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
		+ sp_api::Core<B>
		+ sp_api::Metadata<B>
		+ sp_block_builder::BlockBuilder<B>
		+ sp_transaction_pool::runtime_api::TaggedTransactionQueue<B>
		+ frame_system_rpc_runtime_api::AccountNonceApi<B, AccountId, Nonce>,
{
	pub fn new(client: Arc<C>, pool: Arc<FullPool<B, C>>, pair: sp_core::sr25519::Pair) -> Self {
		Self {
			client,
			pool,
			_phantom_b: Default::default(),
			_phantom_be: Default::default(),
			pair_signer: PairSigner::new(pair),
			runtime_decoders: Arc::new(RwLock::new(HashMap::new())),
			nonce: Arc::new(RwLock::new(None)),
		}
	}

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

	async fn clear_nonce(&self) {
		let mut current_nonce = self.nonce.write().await;
		*current_nonce = None;
	}

	/// Returns the `RuntimeDecoder` at the given block. first it acquires the runtime version at
	/// the given block then it returns a reference to an `RuntimeDecoder` from `runtime_decoders`
	/// hash map. If not found it creates a new one and inserts it inside the map for future quick
	/// access
	async fn runtime_decoder(
		&self,
		block_hash: Hash,
	) -> RpcResult<impl Deref<Target = RuntimeDecoder> + '_> {
		let block_spec_version = self.client.runtime_version_at(block_hash)?.spec_version;

		// Acquire a read guard for the runtime_decoders map
		let decoders_read_guard = self.runtime_decoders.read().await;

		// Check if we have an error decoder corresponding to the runtime of the given block
		if decoders_read_guard.contains_key(&block_spec_version) {
			Ok(RwLockReadGuard::map(decoders_read_guard, |decoders_map| {
				decoders_map.get(&block_spec_version).unwrap()
			}))
		} else {
			// Here we need to create a new runtime decoder for the runtime at the given block_hash
			let maybe_new_decoder = self
				.client
				.runtime_api()
				.metadata_at_version(block_hash, 15)
				.expect("Version 15 should be supported by the runtime.")
				.map(RuntimeDecoder::new);

			// Upgrade the read guard to a write guard
			drop(decoders_read_guard);
			let mut decoders_write_guard = self.runtime_decoders.write().await;

			// Insert the new RuntimeDecoder. If anything goes wrong while creating it, return or
			// insert a default build time RuntimeDecoder with the build runtime_version as key
			let new_decoder_key = match maybe_new_decoder {
				Some(new_runtime_decoder) => {
					decoders_write_guard.entry(block_spec_version).or_insert(new_runtime_decoder);
					block_spec_version
				},
				None => {
					let default_spec_version = build_runtime_version().spec_version;
					decoders_write_guard
						.entry(default_spec_version)
						.or_insert(RuntimeDecoder::default());
					default_spec_version
				},
			};

			// Downgrade the write guard to a read guard and return the reference
			drop(decoders_write_guard);
			Ok(RwLockReadGuard::map(self.runtime_decoders.read().await, |decoders_map| {
				decoders_map.get(&new_decoder_key).unwrap()
			}))
		}
	}

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

	async fn get_extrinsic_data(
		&self,
		block_hash: Hash,
		extrinsic_index: usize,
	) -> RpcResult<ExtrinsicData> {
		let Some(signed_block) = self.client.block(block_hash)? else {
			Err(CfApiError::OtherError(anyhow!("The signed block this transaction was not found")))?
		};
		let Some(extrinsic) = signed_block.block.extrinsics().get(extrinsic_index) else {
			Err(CfApiError::OtherError(anyhow!(
				"The signed block extrinsics does not have an extrinsic at index {:?}",
				extrinsic_index
			)))?
		};

		// Construct the storage key for system events
		let events_storage_key = sp_core::storage::StorageKey(
			frame_support::storage::storage_prefix(b"System", b"Events").to_vec(),
		);

		let raw_events = self.client.storage(block_hash, &events_storage_key)?.map(|v| v.0);

		let dynamic_events = self
			.runtime_decoder(block_hash)
			.await?
			.decode_extrinsic_events(extrinsic_index, raw_events)?;

		let tx_hash =
			<state_chain_runtime::Runtime as frame_system::Config>::Hashing::hash_of(extrinsic);

		match dynamic_events.extrinsic_result()? {
			Either::Left(dispatch_info) =>
				Ok((tx_hash, dynamic_events, signed_block.block.header().clone(), dispatch_info)),
			Either::Right(dispatch_error) => Err(CfApiError::ExtrinsicDispatchError(
				self.runtime_decoder(block_hash).await?.decode_dispatch_error(dispatch_error),
			)),
		}
	}

	async fn get_extrinsic_details(
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
		let result = extrinsic_events
			.iter()
			.find_map(|event| match event {
				state_chain_runtime::RuntimeEvent::System(
					frame_system::Event::ExtrinsicSuccess { dispatch_info },
				) => Some(Ok(*dispatch_info)),
				state_chain_runtime::RuntimeEvent::System(
					frame_system::Event::ExtrinsicFailed { dispatch_error, dispatch_info: _ },
				) => Some(Err(*dispatch_error)),
				_ => None,
			})
			.expect("Unexpected state chain node behaviour")
			.map(|dispatch_info| {
				(tx_hash, extrinsic_events, signed_block.block.header().clone(), dispatch_info)
			});

		match result {
			Ok(details) => Ok(details),
			Err(dispatch_error) => Err(CfApiError::ExtrinsicDispatchError(
				self.runtime_decoder(block_hash).await?.decode_dispatch_error(dispatch_error),
			)),
		}
	}

	async fn handle_transaction_pool_error(
		&self,
		pool_error: sc_transaction_pool::error::Error,
	) -> RpcResult<()> {
		match pool_error {
			sc_transaction_pool::error::Error::Pool(
				sc_transaction_pool_api::error::Error::TooLowPriority { .. },
			) => {
				// This occurs when a transaction with the same nonce is in the transaction pool
				// and the priority is <= priority of that existing tx
				log::warn!(
					"TooLowPriority error. More likely occurs when a transaction with the same nonce \
					 is in the transaction pool. Resetting the pool_client managed nonce ..."
				);
				self.clear_nonce().await;
			},
			sc_transaction_pool::error::Error::Pool(
				sc_transaction_pool_api::error::Error::InvalidTransaction(
					sp_runtime::transaction_validity::InvalidTransaction::Stale,
				),
			) => {
				// This occurs when the nonce has already been *consumed* i.e
				// a transaction with that nonce is in a block
				log::warn!(
					"InvalidTransaction::Stale error, more likely none too low. Resetting \
					 the pool_client managed nonce ..."
				);
				self.clear_nonce().await;
			},
			sc_transaction_pool::error::Error::Pool(
				sc_transaction_pool_api::error::Error::InvalidTransaction(
					sp_runtime::transaction_validity::InvalidTransaction::BadProof,
				),
			) => {
				// This occurs when the extra details used to sign the extrinsic such as the
				// runtimeVersion are different from the verification side
				log::warn!(
					"InvalidTransaction::BadProof error, more likely due to RuntimeVersion mismatch. \
					 Resubmitting with the new runtime_version ..."
				);
			},
			_ => {
				return Err(pool_error.into());
			},
		}
		Ok(())
	}

	/// Uses the `BlockBuilder` trait `apply_extrinsic` function to dry run the extrinsic
	/// This is the same function used by Polkadot System api rpc call `system_dryRun`.
	/// Meant to be used to quickly test if an extrinsic would result in a failure. Note that this
	/// always uses the current account nonce at the best block.
	async fn dry_run_extrinsic(&self, call: RuntimeCall) -> RpcResult<()> {
		let best_block = self.client.info().best_hash;

		// For apply_extrinsic call, always uses the current stored account nonce.
		// Using the signed_pool_client managed nonce, might result in apply_extrinsic Future error
		// when the signed_pool_client managed nonce is higher than the current account nonce
		let account_nonce = self
			.client
			.runtime_api()
			.account_nonce(best_block, self.pair_signer.account_id.clone())?;

		let extrinsic = self.create_signed_extrinsic(call, account_nonce)?;

		match self.client.runtime_api().apply_extrinsic(best_block, extrinsic)? {
			Ok(dispatch_result) => match dispatch_result {
				Ok(_) => Ok(()),
				Err(dispatch_error) => Err(CfApiError::ExtrinsicDispatchError(
					self.runtime_decoder(best_block).await?.decode_dispatch_error(dispatch_error),
				)),
			},
			Err(e) => Err(e.into()),
		}
	}

	/// Signs and submits and a `RuntimeCall` to the transaction pool.
	/// Depending on the `wait_for` param:
	/// * `WaitFor::NoWait`: submits extrinsic and returns the transaction hash without watching for
	///   its progress
	/// * `WaitFor::InBlock`: submits extrinsic and waits until the transaction is in a block
	/// * `WaitFor::Finalized`: submits extrinsic and waits until the transaction is in a finalized
	///   block
	pub async fn submit_wait_for_result(
		&self,
		call: RuntimeCall,
		wait_for: WaitFor,
		dry_run: bool,
	) -> RpcResult<WaitForResult> {
		match wait_for {
			WaitFor::NoWait =>
				Ok(WaitForResult::TransactionHash(self.submit_one(call, dry_run).await?)),
			WaitFor::InBlock =>
				Ok(WaitForResult::Details(self.submit_watch(call, false, dry_run).await?)),
			WaitFor::Finalized =>
				Ok(WaitForResult::Details(self.submit_watch(call, true, dry_run).await?)),
		}
	}

	pub async fn submit_wait_for_result_dynamic(
		&self,
		call: RuntimeCall,
		wait_for: WaitFor,
		dry_run: bool,
	) -> RpcResult<WaitForDynamicResult> {
		match wait_for {
			WaitFor::NoWait =>
				Ok(WaitForDynamicResult::TransactionHash(self.submit_one(call, dry_run).await?)),
			WaitFor::InBlock => Ok(WaitForDynamicResult::Data(
				self.submit_watch_dynamic(call, false, dry_run).await?,
			)),
			WaitFor::Finalized => Ok(WaitForDynamicResult::Data(
				self.submit_watch_dynamic(call, true, dry_run).await?,
			)),
		}
	}

	/// Signs and submits a `RuntimeCall` to the transaction pool without watching for its progress.
	/// if successful, it returns the transaction hash otherwise returns a CallError
	pub async fn submit_one(&self, call: RuntimeCall, dry_run: bool) -> RpcResult<Hash> {
		if dry_run {
			self.dry_run_extrinsic(call.clone()).await?;
		}
		loop {
			let at_block = self.client.info().best_hash;

			let extrinsic =
				self.create_signed_extrinsic(call.clone(), self.next_nonce(at_block).await?)?;

			match self.pool.submit_one(at_block, TransactionSource::External, extrinsic).await {
				Ok(tx_hash) => return Ok(tx_hash),
				Err(e) => {
					self.handle_transaction_pool_error(e).await?;
				},
			}
		}
	}

	async fn submit_watch_generic<T, Fut, F>(
		&self,
		call: RuntimeCall,
		until_finalized: bool,
		dry_run: bool,
		get_extrinsic_fn: F,
	) -> RpcResult<T>
	where
		Fut: Future<Output = RpcResult<T>> + Send,
		F: Fn(Hash, usize) -> Fut + Send,
	{
		if dry_run {
			self.dry_run_extrinsic(call.clone()).await?;
		}

		loop {
			let at_block = self.client.info().best_hash;
			let extrinsic =
				self.create_signed_extrinsic(call.clone(), self.next_nonce(at_block).await?)?;

			match self
				.pool
				.submit_and_watch(at_block, TransactionSource::External, extrinsic)
				.await
			{
				Ok(mut status_stream) => {
					// Periodically poll the transaction pool to check inclusion status
					while let Some(status) = status_stream.next().await {
						match status {
							TransactionStatus::InBlock((block_hash, tx_index)) =>
								if !until_finalized {
									return get_extrinsic_fn(block_hash, tx_index).await;
								},
							TransactionStatus::Finalized((block_hash, tx_index)) =>
								if until_finalized {
									return get_extrinsic_fn(block_hash, tx_index).await;
								},
							TransactionStatus::Future |
							TransactionStatus::Ready |
							TransactionStatus::Broadcast(_) => continue,
							TransactionStatus::Invalid => {
								return Err(CfApiError::OtherError(anyhow!(
									"transaction is no longer valid in the current state. "
								)))
							},
							TransactionStatus::Dropped => {
								return Err(CfApiError::OtherError(anyhow!(
									"transaction was dropped from the pool because of the limit"
								)))
							},
							TransactionStatus::Usurped(_hash) => {
								return Err(CfApiError::OtherError(anyhow!(
									"Transaction has been replaced in the pool, "
								)))
							},
							TransactionStatus::FinalityTimeout(_block_hash) => {
								//return Err(CfApiError::OtherError(anyhow!("Maximum number of
								// finality watchers has been reached")))
								continue
							},
							TransactionStatus::Retracted(_block_hash) => {
								Err(CfApiError::OtherError(anyhow!("The block this transaction was included in has been retracted.")))?
							},
						}
					}
					return Err(CfApiError::OtherError(anyhow!("transaction unexpected error")))
				},
				Err(e) => {
					self.handle_transaction_pool_error(e).await?;
				},
			};
		}
	}

	/// Signs and submits a `RuntimeCall` to the transaction pool and watches its progress. Once the
	/// extrinsic is in a block, `ExtrinsicDetails` is returned. `ExtrinsicDetails` contains
	/// static events (event types known at compile time) of type
	/// `state_chain_runtime::RuntimeEvent`. This means if a runtime upgrade changes an event
	/// signature, this function can't decode the changed event anymore. Use the alternative
	/// `submit_watch_dynamic` to be able to dynamically decode events.
	/// `until_finalized` param determines whether to wait until the extrinsic is in a block or in
	/// a finalized block. This is a blocking call, if until_finalized == false it takes around 1
	/// block and if until_finalized == true it takes >2 blocks
	pub async fn submit_watch(
		&self,
		call: RuntimeCall,
		until_finalized: bool,
		dry_run: bool,
	) -> RpcResult<ExtrinsicDetails> {
		self.submit_watch_generic(call, until_finalized, dry_run, |block_hash, tx_index| {
			self.get_extrinsic_details(block_hash, tx_index)
		})
		.await
	}

	/// Signs and submits a `RuntimeCall` to the transaction pool and watches its progress. Once the
	/// extrinsic is in a block, `ExtrinsicData` is returned. `ExtrinsicData` contains dynamic
	/// events of type `DynamicEvents`, these events are decoded dynamically using the current
	/// runtime metadata. This means that if a runtime upgrade changes the event signature, this
	/// function can decode the changed event.
	/// `until_finalized` param determines whether to wait until the extrinsic is in a block or in
	/// a finalized block. This is a blocking call, if until_finalized == false it takes around 1
	/// block and if until_finalized == true it takes >2 blocks
	pub async fn submit_watch_dynamic(
		&self,
		call: RuntimeCall,
		until_finalized: bool,
		dry_run: bool,
	) -> RpcResult<ExtrinsicData> {
		self.submit_watch_generic(call, until_finalized, dry_run, |block_hash, tx_index| {
			self.get_extrinsic_data(block_hash, tx_index)
		})
		.await
	}
}
