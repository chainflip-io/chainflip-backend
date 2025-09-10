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

use crate::CfApiError;
use cf_rpc_apis::{call_error, internal_error, CfErrorCode, NotificationBehaviour, RpcApiError};
use futures::{stream, stream::StreamExt, FutureExt};
use jsonrpsee::{types::error::ErrorObjectOwned, PendingSubscriptionSink, RpcModule};

use cf_primitives::BlockNumber;
use jsonrpsee::tokio::sync::Mutex;
use sc_client_api::{
	blockchain::HeaderMetadata, Backend, BlockBackend, BlockchainEvents, ExecutorProvider,
	HeaderBackend, StorageProvider,
};
use sc_rpc_spec_v2::chain_head::{
	api::ChainHeadApiServer, ChainHead, ChainHeadConfig, FollowEvent,
};
use serde::Serialize;
use sp_api::CallApiAt;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use state_chain_runtime::{
	chainflip::{get_header_timestamp, BlockUpdate},
	runtime_apis::CustomRuntimeApi,
	Hash, Header,
};
use std::{marker::PhantomData, num::NonZero, sync::Arc};

/// The CustomRpcBackend struct provides common logic implementation for providing RPC endpoints.
///
/// It offers methods to interact with the runtime API, manage subscriptions, and handle block
/// notifications, supporting different notification behaviors (finalized, best, new).
pub struct CustomRpcBackend<C, B, BE> {
	pub client: Arc<C>,
	pub backend: Arc<BE>,
	pub executor: Arc<dyn sp_core::traits::SpawnNamed>,
	_phantom: PhantomData<B>,
}

impl<C, B, BE> CustomRpcBackend<C, B, BE>
where
	B: BlockT<Hash = state_chain_runtime::Hash>,
	C: Send + Sync + 'static + HeaderBackend<B>,
{
	pub fn new(
		client: Arc<C>,
		backend: Arc<BE>,
		executor: Arc<dyn sp_core::traits::SpawnNamed>,
	) -> Self {
		Self { client, backend, executor, _phantom: Default::default() }
	}

	pub fn unwrap_or_best(&self, from_rpc: Option<<B as BlockT>::Hash>) -> B::Hash {
		from_rpc.unwrap_or_else(|| self.client.info().best_hash)
	}
}

impl<C, B, BE> CustomRpcBackend<C, B, BE>
where
	B: BlockT<Hash = state_chain_runtime::Hash>,
	C: Send + Sync + 'static + HeaderBackend<B> + sp_api::ProvideRuntimeApi<B>,
{
	pub fn with_runtime_api<E, R>(
		&self,
		at: Option<Hash>,
		f: impl FnOnce(&C::Api, Hash) -> Result<R, E>,
	) -> Result<R, RpcApiError>
	where
		CfApiError: From<E>,
	{
		Ok(f(&*self.client.runtime_api(), self.unwrap_or_best(at)).map_err(CfApiError::from)?)
	}

	pub fn with_versioned_runtime_api<E, R>(
		&self,
		at: Option<Hash>,
		f: impl FnOnce(&C::Api, Hash, Option<u32>) -> Result<R, E>,
	) -> Result<R, RpcApiError>
	where
		CfApiError: From<E>,
	{
		use sp_api::ApiExt;
		self.with_runtime_api::<CfApiError, _>(at, |api, hash| {
			let api_version =
				api.api_version::<dyn CustomRuntimeApi<state_chain_runtime::Block>>(hash)?;

			Ok(f(api, hash, api_version)?)
		})
	}
}

/// Number of pinned blocks before starting to unpin oldest block. Must be lower than
/// `ChainHeadConfig` maximum pinned blocks across all connections (ChainHeadConfig default is 512).
const MAX_RETAINED_PINNED_BLOCKS: usize = 64;

/// A subscription garbage collector that keeps track of pinned block hashes as reported by
/// `chainHead_v1_follow` and unpins old blocks when the number of tracked pinned hashes reaches its
/// configured capacity. The chain head API pins blocks in memory but clients have to explicitly
/// call `chainHead_v1_unpin` to release memory. If pinned blocks reach MAX_PINNED_BLOCKS a `stop`
/// event is generated causing all subscriptions to be dropped.
/// SubscriptionCleaner is per subscription since blocks are pinned only in the context of a
/// specific subscription. The api states that if multiple chainHead_v1_follow subscriptions exist,
/// then each (subscription, block) tuple must be unpinned individually.
struct SubscriptionCleaner<B: BlockT, BE: Backend<B>, C> {
	chain_head_client: Arc<RpcModule<ChainHead<BE, B, C>>>,
	sub_id: String,
	pinned_hashes: Arc<Mutex<lru::LruCache<Hash, ()>>>,
}

impl<B: BlockT, BE: Backend<B>, C> Clone for SubscriptionCleaner<B, BE, C> {
	fn clone(&self) -> Self {
		Self {
			chain_head_client: self.chain_head_client.clone(),
			sub_id: self.sub_id.clone(),
			pinned_hashes: self.pinned_hashes.clone(),
		}
	}
}

impl<B: BlockT, BE: Backend<B>, C> SubscriptionCleaner<B, BE, C> {
	pub fn new(
		chain_head_client: Arc<RpcModule<ChainHead<BE, B, C>>>,
		sub_id: String,
		capacity: usize,
	) -> Self {
		Self {
			chain_head_client,
			sub_id,
			pinned_hashes: Arc::new(Mutex::new(lru::LruCache::new(
				NonZero::new(if capacity == 0 { 1 } else { capacity }).unwrap(),
			))),
		}
	}

	pub async fn add(&self, new_hashes: &[Hash]) {
		let old_hashes = {
			// Ensure that the lock is released before the `await` point.
			let mut pinned_blocks = self.pinned_hashes.lock().await;
			new_hashes
				.iter()
				.filter_map(|new_hash| match pinned_blocks.push(*new_hash, ()) {
					// If the returned key is different from the new hash, it indicates that an
					// eviction occurred in the LRU cache.
					Some((old_hash, _)) if old_hash != *new_hash => Some(old_hash),

					// If the returned key is the same as the new hash, it indicates that the block
					// already exists in the cache, and it is just moved to the head of the queue.
					Some(_) => None,

					// If the lru.push returns None, it indicates that the block is not present in
					// the cache. It is simply added and no eviction occurred (cache has capacity).
					None => None,
				})
				.collect::<Vec<_>>()
		};

		if !old_hashes.is_empty() {
			if let Err(e) = self
				.chain_head_client
				.call::<_, ()>(
					"chainHead_v1_unpin",
					jsonrpsee::rpc_params!(&self.sub_id, &old_hashes),
				)
				.await
			{
				log::warn!("Failed to unpin blocks for subscription: {:?} , {:?}", &self.sub_id, e);
			}
		}
	}
}

impl<C, B, BE> CustomRpcBackend<C, B, BE>
where
	B: BlockT<Hash = state_chain_runtime::Hash, Header = state_chain_runtime::Header>,
	B::Header: Unpin,
	BE: Send + Sync + 'static + Backend<B>,
	C: sp_api::ProvideRuntimeApi<B>
		+ Send
		+ Sync
		+ 'static
		+ BlockBackend<B>
		+ ExecutorProvider<B>
		+ HeaderBackend<B>
		+ HeaderMetadata<B, Error = sc_client_api::blockchain::Error>
		+ BlockchainEvents<B>
		+ CallApiAt<B>
		+ StorageProvider<B, BE>,
	C::Api: CustomRuntimeApi<B>,
{
	pub fn header_for(&self, block_hash: Hash) -> Result<Header, CfApiError> {
		self.client
			.header(block_hash)
			.map_err(|e| call_error(e, CfErrorCode::OtherError))?
			.ok_or_else(|| CfApiError::HeaderNotFoundError(block_hash))
	}

	pub fn block_number_for(&self, block_hash: Hash) -> Result<BlockNumber, CfApiError> {
		Ok(self.header_for(block_hash)?.number)
	}

	fn chain_head_api(&self) -> RpcModule<ChainHead<BE, B, C>> {
		ChainHead::new(
			self.client.clone(),
			self.backend.clone(),
			self.executor.clone(),
			ChainHeadConfig::default(),
		)
		.into_rpc()
	}

	pub async fn new_subscription<
		T: Serialize + Send + Clone + Eq + 'static,
		F: Fn(&C, state_chain_runtime::Hash) -> Result<T, RpcApiError> + Send + Clone + 'static,
	>(
		&self,
		notification_behaviour: NotificationBehaviour,
		only_on_changes: bool,
		end_on_error: bool,
		sink: PendingSubscriptionSink,
		f: F,
	) {
		self.new_subscription_with_state(
			notification_behaviour,
			only_on_changes,
			end_on_error,
			sink,
			move |client, hash, _state| f(client, hash).map(|res| (res, ())),
		)
		.await
	}

	/// The subscription will return the first value immediately and then either return new values
	/// only when it changes, or every new block.
	/// Note depending on the notification_behaviour blocks can be skipped. Also this
	/// subscription can either filter out, or end the stream if the provided async closure returns
	/// an error.
	async fn new_subscription_with_state<
		T: Serialize + Send + Clone + Eq + 'static,
		// State to carry forward between calls to the closure.
		S: 'static + Clone + Send,
		F: Fn(&C, state_chain_runtime::Hash, Option<&S>) -> Result<(T, S), RpcApiError>
			+ Send
			+ Clone
			+ 'static,
	>(
		&self,
		notification_behaviour: NotificationBehaviour,
		only_on_changes: bool,
		end_on_error: bool,
		pending_sink: PendingSubscriptionSink,
		f: F,
	) {
		// Make sure to use the same chain head client for both subscription and cleaner
		let chain_head_client = Arc::new(self.chain_head_api());

		let Ok(subscription) = chain_head_client
			.clone()
			.subscribe_unbounded("chainHead_v1_follow", [false])
			.await
		else {
			pending_sink
				.reject(internal_error("chainHead_v1_follow subscription failed"))
				.await;
			return;
		};

		let Ok(subscription_id) = serde_json::to_string(&subscription.subscription_id()) else {
			pending_sink
				.reject(internal_error(format!(
					"Unable to serialize subscription id {:?}",
					subscription.subscription_id()
				)))
				.await;
			return;
		};

		// construct either best, new or finalized blocks stream from the chain head subscription
		let blocks_stream = stream::unfold(
			(
				subscription,
				SubscriptionCleaner::new(
					chain_head_client,
					subscription_id,
					MAX_RETAINED_PINNED_BLOCKS,
				),
			),
			move |(mut sub, sub_gc)| async move {
				match sub.next::<FollowEvent<Hash>>().await {
					Some(Ok((event, _subs_id))) => Some(((event, sub_gc.clone()), (sub, sub_gc))),
					Some(Err(e)) => {
						log::warn!("ChainHead subscription error {:?}", e);
						None
					},
					_ => None,
				}
			},
		)
		.filter_map(move |(event, sub_gc)| async move {
			// The finalized blocks reported in the initialized event and each subsequent block
			// reported with a newBlock event, are pinned by the chain head API. These blocks
			// need to be added to the subscription's garbage collector for later unpinning.
			match event {
				FollowEvent::Initialized(sc_rpc_spec_v2::chain_head::Initialized {
					ref finalized_block_hashes,
					..
				}) => sub_gc.add(finalized_block_hashes).await,
				FollowEvent::NewBlock(sc_rpc_spec_v2::chain_head::NewBlock {
					ref block_hash,
					..
				}) => sub_gc.add(&[*block_hash]).await,
				FollowEvent::Finalized(sc_rpc_spec_v2::chain_head::Finalized {
					ref finalized_block_hashes,
					..
				}) => sub_gc.add(finalized_block_hashes).await,
				_ => {},
			}

			// When NotificationBehaviour is:
			// * NotificationBehaviour::Finalized: listen to initialized and finalized events
			// * NotificationBehaviour::Best: listen to just bestBlockChanged events
			// * NotificationBehaviour::New: listen to just newBlock events
			// See: https://paritytech.github.io/json-rpc-interface-spec/api/chainHead_v1_follow.html
			match (notification_behaviour, event) {
				(
					// Always start from the most recent finalized block hash
					NotificationBehaviour::Finalized,
					FollowEvent::Initialized(sc_rpc_spec_v2::chain_head::Initialized {
						mut finalized_block_hashes,
						..
					}),
				) => Some(vec![finalized_block_hashes
					.pop()
					.expect("Guaranteed to have at least one element.")]),
				(
					NotificationBehaviour::Finalized,
					FollowEvent::Finalized(sc_rpc_spec_v2::chain_head::Finalized {
						finalized_block_hashes,
						..
					}),
				) => Some(finalized_block_hashes),
				(
					NotificationBehaviour::Best,
					FollowEvent::BestBlockChanged(sc_rpc_spec_v2::chain_head::BestBlockChanged {
						best_block_hash,
					}),
				) => Some(vec![best_block_hash]),
				(
					NotificationBehaviour::New,
					FollowEvent::NewBlock(sc_rpc_spec_v2::chain_head::NewBlock {
						block_hash, ..
					}),
				) => Some(vec![block_hash]),
				(_, FollowEvent::Stop) => {
					log::warn!("ChainHead subscription {:?} received a STOP event.", sub_gc.sub_id);
					None
				},
				_ => None,
			}
		})
		.map(stream::iter)
		.flatten();

		let stream = blocks_stream
			.filter_map({
				let client = self.client.clone();

				let mut previous_item = None;
				let mut previous_state = None;

				move |hash| {
					futures::future::ready(match f(&client, hash, previous_state.as_ref()) {
						Ok((new_item, new_state))
							if !only_on_changes || Some(&new_item) != previous_item.as_ref() =>
						{
							previous_item = Some(new_item.clone());
							previous_state = Some(new_state);

							match client.header(hash) {
								Ok(Some(header)) => Some(Ok(BlockUpdate {
									block_hash: hash,
									block_number: *header.number(),
									timestamp: get_header_timestamp(&header).unwrap_or_default(),
									data: new_item,
								})),
								Ok(None) =>
									if end_on_error {
										Some(Err(internal_error(format!(
											"Could not fetch block header for block {:?}",
											hash
										))))
									} else {
										None
									},
								Err(e) =>
									if end_on_error {
										Some(Err(internal_error(format!(
											"Couldn't fetch block header for block {:?}: {}",
											hash, e
										))))
									} else {
										None
									},
							}
						},
						Err(error) => {
							log::warn!("Subscription Error: {error}.");
							if end_on_error {
								log::warn!("Closing Subscription.");
								Some(Err(ErrorObjectOwned::from(error)))
							} else {
								None
							}
						},
						_ => None,
					})
				}
			})
			.take_while(|item| futures::future::ready(item.is_ok()))
			.map(Result::unwrap)
			.boxed();

		self.executor.spawn(
			"cf-rpc-update-subscription",
			Some("rpc"),
			sc_rpc::utils::pipe_from_stream(pending_sink, stream).boxed(),
		);
	}
}
