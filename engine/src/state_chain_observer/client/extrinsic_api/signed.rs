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

use std::sync::Arc;

use super::{
	super::{
		base_rpc_api,
		stream_api::{StreamApi, FINALIZED},
	},
	common::send_request,
};

use anyhow::Result;
use async_trait::async_trait;
use cf_node_client::{signer, WaitForResult};
use cf_primitives::WaitFor;
use cf_utilities::task_scope::{task_scope, Scope, ScopedJoinHandle, UnwrapOrCancel};
use futures::StreamExt;
use futures_util::FutureExt;
use sp_core::H256;
use state_chain_runtime::{AccountId, Nonce};
use tokio::sync::{mpsc, oneshot};
use tracing::trace;

use crate::constants::SIGNED_EXTRINSIC_LIFETIME;

mod submission_watcher;

// Wrapper type to avoid await.await on submits/finalize calls being possible
#[cfg_attr(any(test, feature = "client-mocks"), mockall::automock)]
#[async_trait]
pub trait UntilFinalized {
	async fn until_finalized(self) -> submission_watcher::FinalizationResult;
}
#[async_trait]
impl<W: UntilFinalized + Send> UntilFinalized for (state_chain_runtime::Hash, W) {
	async fn until_finalized(self) -> submission_watcher::FinalizationResult {
		self.1.until_finalized().await
	}
}
#[async_trait]
impl<T: UntilInBlock + Send, W: UntilFinalized + Send> UntilFinalized for (T, W) {
	async fn until_finalized(self) -> submission_watcher::FinalizationResult {
		self.1.until_finalized().await
	}
}

pub struct UntilFinalizedFuture(oneshot::Receiver<submission_watcher::FinalizationResult>);
#[async_trait]
impl UntilFinalized for UntilFinalizedFuture {
	async fn until_finalized(self) -> submission_watcher::FinalizationResult {
		self.0.unwrap_or_cancel().await
	}
}

// Wrapper type to avoid await.await on submits/finalize calls being possible
#[cfg_attr(any(test, feature = "client-mocks"), mockall::automock)]
#[async_trait]
pub trait UntilInBlock {
	async fn until_in_block(self) -> submission_watcher::InBlockResult;
}
#[async_trait]
impl<W: UntilInBlock + Send> UntilInBlock for (state_chain_runtime::Hash, W) {
	async fn until_in_block(self) -> submission_watcher::InBlockResult {
		self.1.until_in_block().await
	}
}
#[async_trait]
impl<T: UntilFinalized + Send, W: UntilInBlock + Send> UntilInBlock for (W, T) {
	async fn until_in_block(self) -> submission_watcher::InBlockResult {
		self.0.until_in_block().await
	}
}
pub struct UntilInBlockFuture(oneshot::Receiver<submission_watcher::InBlockResult>);
#[async_trait]
impl UntilInBlock for UntilInBlockFuture {
	async fn until_in_block(self) -> submission_watcher::InBlockResult {
		self.0.unwrap_or_cancel().await
	}
}

// Note 'static on the generics in this trait are only required for mockall to mock it
#[async_trait]
pub trait SignedExtrinsicApi {
	type UntilFinalizedFuture: UntilFinalized + Send;
	type UntilInBlockFuture: UntilInBlock + Send;

	fn account_id(&self) -> AccountId;

	async fn submit_signed_extrinsic<Call>(
		&self,
		call: Call,
	) -> (H256, (Self::UntilInBlockFuture, Self::UntilFinalizedFuture))
	where
		Call: Into<state_chain_runtime::RuntimeCall>
			+ Clone
			+ std::fmt::Debug
			+ Send
			+ Sync
			+ 'static;

	async fn submit_signed_extrinsic_wait_for<Call>(
		&self,
		call: Call,
		wait_for: WaitFor,
	) -> Result<WaitForResult>
	where
		Call: Into<state_chain_runtime::RuntimeCall>
			+ Clone
			+ std::fmt::Debug
			+ Send
			+ Sync
			+ 'static;

	async fn submit_signed_extrinsic_with_dry_run<Call>(
		&self,
		call: Call,
	) -> Result<(H256, (Self::UntilInBlockFuture, Self::UntilFinalizedFuture))>
	where
		Call: Into<state_chain_runtime::RuntimeCall>
			+ Clone
			+ std::fmt::Debug
			+ Send
			+ Sync
			+ 'static;

	async fn finalize_signed_extrinsic<Call>(
		&self,
		call: Call,
	) -> (Self::UntilInBlockFuture, Self::UntilFinalizedFuture)
	where
		Call: Into<state_chain_runtime::RuntimeCall>
			+ Clone
			+ std::fmt::Debug
			+ Send
			+ Sync
			+ 'static;
}

pub struct SignedExtrinsicClient {
	account_id: AccountId,
	request_sender: mpsc::Sender<(
		state_chain_runtime::RuntimeCall,
		oneshot::Sender<submission_watcher::InBlockResult>,
		oneshot::Sender<submission_watcher::FinalizationResult>,
		submission_watcher::RequestStrategy,
	)>,
	dry_run_sender: mpsc::Sender<(state_chain_runtime::RuntimeCall, oneshot::Sender<Result<()>>)>,
	_task_handle: ScopedJoinHandle<()>,
}

impl SignedExtrinsicClient {
	pub async fn new<
		BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static,
		BlockStream: StreamApi<FINALIZED> + Clone,
	>(
		scope: &Scope<'_, anyhow::Error>,
		base_rpc_client: Arc<BaseRpcClient>,
		account_nonce: Nonce,
		signer: signer::PairSigner<sp_core::sr25519::Pair>,
		genesis_hash: H256,
		state_chain_stream: &mut BlockStream,
	) -> Result<Self> {
		const REQUEST_BUFFER: usize = 16;

		let (request_sender, mut request_receiver) = mpsc::channel(REQUEST_BUFFER);
		let (dry_run_sender, mut dry_run_receiver) = mpsc::channel(REQUEST_BUFFER);

		Ok(Self {
			account_id: signer.account_id.clone(),
			request_sender,
			dry_run_sender,
			_task_handle: scope.spawn_with_handle({
				let mut state_chain_stream = state_chain_stream.clone();

				task_scope(move |scope| async move {
					let (mut submission_watcher, mut requests) =
						submission_watcher::SubmissionWatcher::new(
							scope,
							signer,
							account_nonce,
							state_chain_stream.cache().hash,
							state_chain_stream.cache().number,
							base_rpc_client.runtime_version(None).await?,
							genesis_hash,
							SIGNED_EXTRINSIC_LIFETIME,
							base_rpc_client.clone(),
						);

					cf_utilities::loop_select! {
						if let Some((call, until_in_block_sender, until_finalized_sender, strategy)) = request_receiver.recv() => {
							submission_watcher.new_request(&mut requests, call, until_in_block_sender, until_finalized_sender, strategy).await?;
						} else break Ok(()),
						if let Some((call, result_sender)) = dry_run_receiver.recv() => {
							let _ = result_sender.send(submission_watcher.dry_run_extrinsic(call).await.map_err(Into::into));
						} else break Ok(()),
						let submission_details = submission_watcher.watch_for_submission_in_block() => {
							submission_watcher.on_submission_in_block(&mut requests, submission_details).await?;
						},
						if let Some(block) = state_chain_stream.next() => {
							trace!("Received state chain block: {} ({:x?})", block.number, block.hash);
							submission_watcher.on_block_finalized(
								&mut requests,
								block.hash,
							).await?;
						} else break Ok(()),
					}
				}.boxed())
			}),
		})
	}
}

#[async_trait]
impl SignedExtrinsicApi for SignedExtrinsicClient {
	type UntilFinalizedFuture = UntilFinalizedFuture;
	type UntilInBlockFuture = UntilInBlockFuture;

	fn account_id(&self) -> AccountId {
		self.account_id.clone()
	}

	async fn submit_signed_extrinsic<Call>(
		&self,
		call: Call,
	) -> (H256, (Self::UntilInBlockFuture, Self::UntilFinalizedFuture))
	where
		Call: Into<state_chain_runtime::RuntimeCall>
			+ Clone
			+ std::fmt::Debug
			+ Send
			+ Sync
			+ 'static,
	{
		let (until_in_block_sender, until_in_block_receiver) = oneshot::channel();
		let (until_finalized_sender, until_finalized_receiver) = oneshot::channel();
		(
			send_request(&self.request_sender, |hash_sender| {
				(
					call.into(),
					until_in_block_sender,
					until_finalized_sender,
					submission_watcher::RequestStrategy::StrictlyOneSubmission(hash_sender),
				)
			})
			.await
			.unwrap_or_cancel()
			.await,
			(
				UntilInBlockFuture(until_in_block_receiver),
				UntilFinalizedFuture(until_finalized_receiver),
			),
		)
	}

	async fn submit_signed_extrinsic_wait_for<Call>(
		&self,
		call: Call,
		wait_for: WaitFor,
	) -> Result<WaitForResult>
	where
		Call: Into<state_chain_runtime::RuntimeCall>
			+ Clone
			+ std::fmt::Debug
			+ Send
			+ Sync
			+ 'static,
	{
		let (hash, (until_in_block, until_finalized)) =
			self.submit_signed_extrinsic(call.clone()).await;

		// can dedup this and put it into details whether in block or finalised
		let extrinsic_data = match wait_for {
			WaitFor::NoWait => return Ok(WaitForResult::TransactionHash(hash)),
			WaitFor::InBlock => until_in_block.until_in_block().await?,
			WaitFor::Finalized => until_finalized.until_finalized().await?,
		};

		Ok(WaitForResult::Details(Box::new(extrinsic_data)))
	}

	/// Dry run the call, and only submit the extrinsic onto the chain
	/// if dry-run returns Ok(())
	async fn submit_signed_extrinsic_with_dry_run<Call>(
		&self,
		call: Call,
	) -> Result<(H256, (Self::UntilInBlockFuture, Self::UntilFinalizedFuture))>
	where
		Call: Into<state_chain_runtime::RuntimeCall>
			+ Clone
			+ std::fmt::Debug
			+ Send
			+ Sync
			+ 'static,
	{
		let _ = send_request(&self.dry_run_sender, |result_sender| {
			(call.clone().into(), result_sender)
		})
		.await
		.unwrap_or_cancel()
		.await?;

		Ok(self.submit_signed_extrinsic(call.into()).await)
	}

	async fn finalize_signed_extrinsic<Call>(
		&self,
		call: Call,
	) -> (Self::UntilInBlockFuture, Self::UntilFinalizedFuture)
	where
		Call: Into<state_chain_runtime::RuntimeCall>
			+ Clone
			+ std::fmt::Debug
			+ Send
			+ Sync
			+ 'static,
	{
		let (until_finalized_sender, until_finalized_receiver) = oneshot::channel();

		(
			UntilInBlockFuture(
				send_request(&self.request_sender, |until_in_block_sender| {
					(
						call.into(),
						until_in_block_sender,
						until_finalized_sender,
						submission_watcher::RequestStrategy::AllowMultipleSubmissions,
					)
				})
				.await,
			),
			UntilFinalizedFuture(until_finalized_receiver),
		)
	}
}
