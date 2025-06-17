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

use crate::StorageQueryApi;
use cf_node_client::{
	error_decoder,
	events_decoder::{self, DynamicEvents},
	runtime_decoder::RuntimeDecoder,
	signer::PairSigner,
	ExtrinsicData, WaitForDynamicResult, WaitForResult,
};
use cf_primitives::WaitFor;
use codec::{Decode, Encode};
use frame_system_rpc_runtime_api::AccountNonceApi;
use futures::StreamExt;
use jsonrpsee::{
	tokio::sync::{RwLock, RwLockReadGuard, Semaphore},
	types::ErrorObjectOwned,
};
use sc_client_api::{
	blockchain::HeaderMetadata, Backend, BlockBackend, HeaderBackend, StorageProvider,
};
use sc_transaction_pool::FullPool;
use sc_transaction_pool_api::{TransactionPool, TransactionStatus, TransactionStatusStreamFor};
use sp_api::{ApiError, CallApiAt, Core, Metadata};
use sp_block_builder::BlockBuilder;
use sp_core::crypto::AccountId32;
use sp_runtime::{
	traits::{Block as BlockT, Hash as HashT},
	transaction_validity::{TransactionSource, TransactionValidityError},
	Either,
};
use state_chain_runtime::{runtime_apis::CustomRuntimeApi, AccountId, Hash, Nonce, RuntimeCall};
use std::{collections::HashMap, marker::PhantomData, ops::Deref, pin::Pin, sync::Arc};
use substrate_frame_rpc_system::{System, SystemApiServer};

const SIGNED_EXTRINSIC_LIFETIME: state_chain_runtime::BlockNumber = 128;
const MAX_POOL_SUBMISSION_RETRIES: usize = 10;

#[derive(thiserror::Error, Debug)]
pub enum PoolClientError {
	#[error("The block for this hash was not found: {0}")]
	BlockNotFound(Hash),
	#[error("The block extrinsics does not have an extrinsic at index {0}")]
	ExtrinsicNotFound(usize),
	#[error("Failed to submit extrinsic to the transaction pool after {0} attempts")]
	PoolSubmitError(usize),
	#[error("Could not acquire lock for transaction pool")]
	PoolLockingError,
	#[error("Unexpected end of stream")]
	UnexpectedEndOfStream,
	#[error("{0:?}")]
	TransactionStatusError(&'static str),

	#[error("{0:?}")]
	CodecError(#[from] codec::Error),
	#[error("{0:?}")]
	RuntimeApiError(#[from] ApiError),
	#[error("{0:?}")]
	SubstrateClientError(#[from] sc_client_api::blockchain::Error),
	#[error("{0:?}")]
	TransactionPoolError(#[from] sc_transaction_pool::error::Error),
	#[error("{0:?}")]
	TransactionValidityError(#[from] TransactionValidityError),
	#[error("{0:?}")]
	ExtrinsicDispatchError(#[from] error_decoder::DispatchError),
	#[error("{0:?}")]
	ExtrinsicDynamicEventsError(#[from] events_decoder::DynamicEventError),
	#[error(transparent)]
	ErrorObject(#[from] ErrorObjectOwned),
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
		+ sp_api::Core<B>
		+ sp_api::Metadata<B>
		+ sp_block_builder::BlockBuilder<B>
		+ sp_transaction_pool::runtime_api::TaggedTransactionQueue<B>
		+ frame_system_rpc_runtime_api::AccountNonceApi<B, AccountId, Nonce>,
{
	client: Arc<C>,
	pool: Arc<FullPool<B, C>>,
	pool_semaphore: Arc<Semaphore>,
	_phantom: PhantomData<(B, BE)>,
	system_api: System<FullPool<B, C>, C, B>,
	pair_signer: PairSigner<sp_core::sr25519::Pair>,
	runtime_decoders: Arc<RwLock<HashMap<u32, RuntimeDecoder>>>,
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
			_phantom: Default::default(),
			system_api: System::new(client.clone(), pool.clone(), sc_rpc_api::DenyUnsafe::Yes),
			pair_signer: PairSigner::new(pair),
			runtime_decoders: Arc::new(RwLock::new(HashMap::new())),
			pool_semaphore: Arc::new(Semaphore::new(1)), // Allow only 1 pool access at a time
			client,
			pool,
		}
	}

	pub fn account_id(&self) -> AccountId32 {
		self.pair_signer.account_id.clone()
	}

	async fn next_nonce(&self) -> Result<Nonce, PoolClientError> {
		// Always return the substrate_frame_rpc_system::System api nonce which takes into
		// consideration all pending transactions currently in the pool
		let system_account_nonce = self.system_api.nonce(self.account_id()).await?;
		Ok(system_account_nonce)
	}

	/// Returns the [RuntimeDecoder] at the given block.
	///
	/// First it acquires the runtime version at the given block then it returns a reference to an
	/// [RuntimeDecoder] from `runtime_decoders` hash map. If not found it creates a new one and
	/// inserts it inside the map for future quick access.
	async fn runtime_decoder(
		&self,
		block_hash: Hash,
	) -> Result<impl Deref<Target = RuntimeDecoder> + '_, PoolClientError> {
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
			let new_runtime_decoder = RuntimeDecoder::new(
				self.client
					.runtime_api()
					.metadata_at_version(block_hash, 15)
					.expect("metadata_at_version Runtime API should be supported")
					.expect("Version 15 should be supported by the runtime."),
			);

			// Upgrade the read guard to a write guard
			drop(decoders_read_guard);
			let mut decoders_write_guard = self.runtime_decoders.write().await;

			decoders_write_guard.insert(block_spec_version, new_runtime_decoder);

			// Downgrade the write guard to a read guard and return the reference
			drop(decoders_write_guard);
			Ok(RwLockReadGuard::map(self.runtime_decoders.read().await, |decoders_map| {
				decoders_map.get(&block_spec_version).unwrap()
			}))
		}
	}

	fn create_signed_extrinsic(
		&self,
		call: RuntimeCall,
		nonce: Nonce,
	) -> Result<B::Extrinsic, PoolClientError> {
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

		// Needs to be returned as an OpaqueExtrinsic, which is just a wrapped `Vec<u8>`
		let call_data = signed_extrinsic.encode();

		Ok(Decode::decode(&mut &call_data[..])?)
	}

	pub async fn get_extrinsic_data_dynamic(
		&self,
		block_hash: Hash,
		extrinsic_index: usize,
	) -> Result<ExtrinsicData<DynamicEvents>, PoolClientError> {
		let signed_block = self
			.client
			.block(block_hash)?
			.ok_or(PoolClientError::BlockNotFound(block_hash))?;

		let extrinsic = signed_block
			.block
			.extrinsics()
			.get(extrinsic_index)
			.ok_or(PoolClientError::ExtrinsicNotFound(extrinsic_index))?;

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
			Either::Left(dispatch_info) => Ok(ExtrinsicData {
				tx_hash,
				events: dynamic_events,
				header: signed_block.block.header().clone(),
				tx_index: extrinsic_index,
				dispatch_info,
				block_hash,
			}),
			Either::Right(dispatch_error) => Err(PoolClientError::ExtrinsicDispatchError(
				self.runtime_decoder(block_hash).await?.decode_dispatch_error(dispatch_error),
			)),
		}
	}

	async fn get_extrinsic_data_static(
		&self,
		block_hash: Hash,
		extrinsic_index: usize,
	) -> Result<ExtrinsicData<Vec<state_chain_runtime::RuntimeEvent>>, PoolClientError> {
		let signed_block = self
			.client
			.block(block_hash)?
			.ok_or(PoolClientError::BlockNotFound(block_hash))?;

		let extrinsic = signed_block
			.block
			.extrinsics()
			.get(extrinsic_index)
			.ok_or(PoolClientError::ExtrinsicNotFound(extrinsic_index))?;

		let block_events = StorageQueryApi::new(&self.client)
			.get_storage_value::<frame_system::Events<state_chain_runtime::Runtime>, _>(block_hash)
			.map_err(|e| PoolClientError::ErrorObject(e.into()))?;

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
			.map(|dispatch_info| ExtrinsicData {
				tx_hash,
				events: extrinsic_events,
				header: signed_block.block.header().clone(),
				tx_index: extrinsic_index,
				dispatch_info,
				block_hash,
			});

		match result {
			Ok(details) => Ok(details),
			Err(dispatch_error) => Err(PoolClientError::ExtrinsicDispatchError(
				self.runtime_decoder(block_hash).await?.decode_dispatch_error(dispatch_error),
			)),
		}
	}

	/// Uses the `BlockBuilder` trait `apply_extrinsic` function to dry run the extrinsic.
	///
	/// This is the same function used by Polkadot System api rpc call `system_dryRun`.
	/// Meant to be used to quickly test if an extrinsic would result in a failure. Note that this
	/// always uses the current account nonce at the best block.
	async fn dry_run_extrinsic(&self, call: RuntimeCall) -> Result<(), PoolClientError> {
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
				Err(dispatch_error) => Err(PoolClientError::ExtrinsicDispatchError(
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
	pub async fn submit_wait_for_result_static(
		&self,
		call: RuntimeCall,
		wait_for: WaitFor,
		dry_run: bool,
	) -> Result<WaitForResult, PoolClientError> {
		match wait_for {
			WaitFor::NoWait =>
				Ok(WaitForResult::TransactionHash(self.submit_one(call, dry_run).await?)),
			WaitFor::InBlock => Ok(WaitForResult::Details(Box::new(
				self.submit_watch_static(call, false, dry_run).await?,
			))),
			WaitFor::Finalized => Ok(WaitForResult::Details(Box::new(
				self.submit_watch_static(call, true, dry_run).await?,
			))),
		}
	}

	pub async fn submit_wait_for_result_dynamic(
		&self,
		call: RuntimeCall,
		wait_for: WaitFor,
		dry_run: bool,
	) -> Result<WaitForDynamicResult, PoolClientError> {
		match wait_for {
			WaitFor::NoWait =>
				Ok(WaitForDynamicResult::TransactionHash(self.submit_one(call, dry_run).await?)),
			WaitFor::InBlock => Ok(WaitForDynamicResult::Data(Box::new(
				self.submit_watch_dynamic(call, false, dry_run).await?,
			))),
			WaitFor::Finalized => Ok(WaitForDynamicResult::Data(Box::new(
				self.submit_watch_dynamic(call, true, dry_run).await?,
			))),
		}
	}

	/// Signs and submits a `RuntimeCall` to the transaction pool without watching for its progress.
	/// if successful, it returns the transaction hash otherwise returns a CallError
	pub async fn submit_one(
		&self,
		call: RuntimeCall,
		dry_run: bool,
	) -> Result<Hash, PoolClientError> {
		if dry_run {
			self.dry_run_extrinsic(call.clone()).await?;
		}

		let _permit = self
			.pool_semaphore
			.acquire()
			.await
			.map_err(|_| PoolClientError::PoolLockingError)?;

		for attempt in 1..=MAX_POOL_SUBMISSION_RETRIES {
			if attempt > 1 {
				log::debug!(
					target: "pool_client",
					"Retrying submit_one to the transaction pool attempt: {attempt} ..."
				);
			}
			let nonce = self.next_nonce().await?;
			let extrinsic = self.create_signed_extrinsic(call.clone(), nonce)?;

			match self
				.pool
				.submit_one(self.client.info().best_hash, TransactionSource::External, extrinsic)
				.await
			{
				Ok(tx_hash) => return Ok(tx_hash),
				Err(pool_error) =>
					if let Some(retry_msg) = is_retriable_pool_error(&pool_error) {
						log::debug!(
							target: "pool_client",
							"Recoverable error: {retry_msg}."
						);
					} else {
						Err(PoolClientError::from(pool_error))?
					},
			}
		}
		Err(PoolClientError::PoolSubmitError(MAX_POOL_SUBMISSION_RETRIES))
	}

	/// Signs and submits a `RuntimeCall` to the transaction pool and watches its progress.
	/// if successful, it returns a `TransactionStatus` stream otherwise returns a PoolClientError
	pub async fn submit_watch(
		&self,
		call: RuntimeCall,
		dry_run: bool,
	) -> Result<Pin<Box<TransactionStatusStreamFor<FullPool<B, C>>>>, PoolClientError> {
		if dry_run {
			self.dry_run_extrinsic(call.clone()).await?;
		}

		let mut maybe_status_stream = None;
		let permit = self
			.pool_semaphore
			.acquire()
			.await
			.map_err(|_| PoolClientError::PoolLockingError)?;

		for attempt in 1..=MAX_POOL_SUBMISSION_RETRIES {
			if attempt > 1 {
				log::debug!(
					target: "pool_client",
					"Retrying submit_and_watch to the transaction pool attempt: {attempt}...",
				);
			}
			let nonce = self.next_nonce().await?;
			let extrinsic = self.create_signed_extrinsic(call.clone(), nonce)?;

			match self
				.pool
				.submit_and_watch(
					self.client.info().best_hash,
					TransactionSource::External,
					extrinsic,
				)
				.await
			{
				Ok(status_stream) => {
					maybe_status_stream = Some(status_stream);
					break;
				},
				Err(pool_error) =>
					if let Some(retry_msg) = is_retriable_pool_error(&pool_error) {
						log::debug!(
							target: "pool_client",
							"Recoverable error: {retry_msg}."
						);
					} else {
						Err(PoolClientError::from(pool_error))?
					},
			};
		}

		// release the semaphore permit as soon as possible
		drop(permit);

		maybe_status_stream.ok_or(PoolClientError::PoolSubmitError(MAX_POOL_SUBMISSION_RETRIES))
	}

	/// Signs and submits a `RuntimeCall` to the transaction pool and watches its progress.
	///
	/// Returns statically decoded events ([state_chain_runtime::RuntimeEvent]).
	///
	/// Static event decoding means that if a runtime upgrade changes an event signature, this
	/// function may fail to decode the changed event. Use the alternative
	/// [SignedPoolClient::submit_watch_dynamic] to be able to dynamically decode events.
	///
	/// See [SignedPoolClient::submit_watch] for more details.
	pub async fn submit_watch_static(
		&self,
		call: RuntimeCall,
		until_finalized: bool,
		dry_run: bool,
	) -> Result<ExtrinsicData<Vec<state_chain_runtime::RuntimeEvent>>, PoolClientError> {
		let mut status_stream = self.submit_watch(call, dry_run).await?;

		// Periodically poll the transaction pool to check inclusion status
		while let Some(status) = status_stream.next().await {
			match status {
				TransactionStatus::InBlock((block_hash, tx_index)) =>
					if !until_finalized {
						return self.get_extrinsic_data_static(block_hash, tx_index).await
					},
				TransactionStatus::Finalized((block_hash, tx_index)) =>
					if until_finalized {
						return self.get_extrinsic_data_static(block_hash, tx_index).await
					},
				_ => is_transaction_status_error(&status)?,
			}
		}
		Err(PoolClientError::UnexpectedEndOfStream)
	}

	/// Signs and submits a `RuntimeCall` to the transaction pool and watches its progress.
	///
	/// Returns dynamically decoded events ([DynamicEvents]).
	///
	/// Dynamic event decoding means that the events are decoded at runtime using the runtime
	/// metadata. This means that if a runtime upgrade changes the event signature, this
	/// function can still decode the changed event.
	///
	/// See [SignedPoolClient::submit_watch] for more details.
	pub async fn submit_watch_dynamic(
		&self,
		call: RuntimeCall,
		until_finalized: bool,
		dry_run: bool,
	) -> Result<ExtrinsicData<DynamicEvents>, PoolClientError> {
		let mut status_stream = self.submit_watch(call, dry_run).await?;

		while let Some(status) = status_stream.next().await {
			match status {
				TransactionStatus::InBlock((block_hash, tx_index)) =>
					if !until_finalized {
						return self.get_extrinsic_data_dynamic(block_hash, tx_index).await
					},
				TransactionStatus::Finalized((block_hash, tx_index)) =>
					if until_finalized {
						return self.get_extrinsic_data_dynamic(block_hash, tx_index).await
					},
				_ => is_transaction_status_error(&status)?,
			}
		}
		Err(PoolClientError::UnexpectedEndOfStream)
	}
}

pub fn is_transaction_status_error(
	status: &TransactionStatus<Hash, Hash>,
) -> Result<(), PoolClientError> {
	match status {
		TransactionStatus::InBlock(_) | TransactionStatus::Finalized(_) => {
			// This should be handled separately
			Ok(())
		},
		TransactionStatus::Future |
		TransactionStatus::Ready |
		TransactionStatus::Broadcast(_) => {
			// Do nothing, just wait for the transaction to be included
			Ok(())
		},
		TransactionStatus::Invalid => {
			Err(PoolClientError::TransactionStatusError(
				"transaction is no longer valid in the current state"
			))
		},
		TransactionStatus::Dropped => {
			Err(PoolClientError::TransactionStatusError(
				"transaction was dropped from the pool because of the limit"
			))
		},
		TransactionStatus::Usurped(_hash) => {
			Err(PoolClientError::TransactionStatusError(
				"Transaction has been replaced in the pool, by another transaction that provides the same tags for example same (sender, nonce)."
			))
		},
		TransactionStatus::FinalityTimeout(_block_hash) => {
			Err(PoolClientError::TransactionStatusError(
				"Maximum number of finality watchers has been reached"
			))
		},
		TransactionStatus::Retracted(_block_hash) => {
			Err(PoolClientError::TransactionStatusError("The block this transaction was included in has been retracted."))
		},
	}
}

fn is_retriable_pool_error(pool_error: &sc_transaction_pool::error::Error) -> Option<&'static str> {
	log::debug!(
		target: "pool_client",
		"Handling pool error: {pool_error}"
	);
	match pool_error {
		sc_transaction_pool::error::Error::Pool(
			sc_transaction_pool_api::error::Error::TooLowPriority { .. },
		) => {
			// This occurs when a transaction with the same nonce is in the transaction pool
			// and the priority is <= priority of that existing tx
			Some(
				"TooLowPriority error. Most likely occurs when a transaction with the same nonce is in the transaction pool",
			)
		},
		sc_transaction_pool::error::Error::Pool(
			sc_transaction_pool_api::error::Error::InvalidTransaction(
				sp_runtime::transaction_validity::InvalidTransaction::Stale,
			),
		) => {
			// This occurs when the nonce has already been *consumed* i.e
			// a transaction with that nonce is in a block
			Some("InvalidTransaction::Stale error, most likely nonce too low")
		},
		sc_transaction_pool::error::Error::Pool(
			sc_transaction_pool_api::error::Error::InvalidTransaction(
				sp_runtime::transaction_validity::InvalidTransaction::BadProof,
			),
		) => {
			// This occurs when the extra details used to sign the extrinsic such as the
			// runtimeVersion are different from the verification side
			Some("InvalidTransaction::BadProof error, most likely due to RuntimeVersion mismatch")
		},
		_ => None,
	}
}
