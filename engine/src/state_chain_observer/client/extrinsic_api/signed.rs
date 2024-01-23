use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use futures_util::FutureExt;
use serde::{Deserialize, Serialize};
use sp_core::H256;
use state_chain_runtime::{AccountId, Nonce};
use tokio::sync::{mpsc, oneshot};
use tracing::trace;
use utilities::task_scope::{task_scope, Scope, ScopedJoinHandle, UnwrapOrCancel};

use crate::constants::SIGNED_EXTRINSIC_LIFETIME;

use self::submission_watcher::ExtrinsicDetails;

use super::{
	super::{
		base_rpc_api,
		stream_api::{StreamApi, FINALIZED},
	},
	common::send_request,
};

pub mod signer;
mod submission_watcher;

// Wrapper type to avoid await.await on submits/finalize calls being possible
#[cfg_attr(test, mockall::automock)]
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
#[cfg_attr(test, mockall::automock)]
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

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
pub enum WaitFor {
	// Return immediately after the extrinsic is submitted
	NoWait,
	// Wait until the extrinsic is included in a block
	InBlock,
	// Wait until the extrinsic is in a finalized block
	#[default]
	Finalized,
}

#[derive(Debug)]
pub enum WaitForResult {
	// The hash of the SC transaction that was submitted.
	TransactionHash(H256),
	Details(ExtrinsicDetails),
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
		'a,
		BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static,
		BlockStream: StreamApi<FINALIZED> + Clone,
	>(
		scope: &Scope<'a, anyhow::Error>,
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
							base_rpc_client.runtime_version().await?,
							genesis_hash,
							SIGNED_EXTRINSIC_LIFETIME,
							base_rpc_client.clone(),
						);

					utilities::loop_select! {
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
		let details = match wait_for {
			WaitFor::NoWait => return Ok(WaitForResult::TransactionHash(hash)),
			WaitFor::InBlock => until_in_block.until_in_block().await?,
			WaitFor::Finalized => until_finalized.until_finalized().await?,
		};

		Ok(WaitForResult::Details(details))
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
