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

use crate::{internal_error, CfApiError, RpcResult};
use futures::{stream, stream::StreamExt, FutureExt};
use jsonrpsee::{types::error::ErrorObjectOwned, PendingSubscriptionSink, RpcModule};

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
use state_chain_runtime::{chainflip::BlockUpdate, runtime_apis::CustomRuntimeApi, Hash};
use std::{marker::PhantomData, sync::Arc};

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
	) -> RpcResult<R>
	where
		CfApiError: From<E>,
	{
		Ok(f(&*self.client.runtime_api(), self.unwrap_or_best(at))?)
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NotificationBehaviour {
	/// Subscription will return finalized blocks.
	Finalized,
	/// Subscription will return best blocks. In the case of a re-org it might drop events.
	#[default]
	Best,
	/// Subscription will return all new blocks. In the case of a re-org it might duplicate events.
	///
	/// The caller is responsible for de-duplicating events.
	New,
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
		F: Fn(&C, state_chain_runtime::Hash) -> Result<T, CfApiError> + Send + Clone + 'static,
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
		F: Fn(&C, state_chain_runtime::Hash, Option<&S>) -> Result<(T, S), CfApiError>
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
		// subscribe to the chain head
		let Ok(subscription) =
			self.chain_head_api().subscribe_unbounded("chainHead_v1_follow", [false]).await
		else {
			pending_sink
				.reject(internal_error("chainHead_v1_follow subscription failed"))
				.await;
			return;
		};

		// construct either best, new or finalized blocks stream from the chain head subscription
		let blocks_stream = stream::unfold(subscription, move |mut sub| async move {
			match sub.next::<FollowEvent<Hash>>().await {
				Some(Ok((event, _subs_id))) => Some((event, sub)),
				Some(Err(e)) => {
					log::warn!("ChainHead subscription error {:?}", e);
					None
				},
				_ => None,
			}
		})
		.filter_map(move |event| async move {
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
