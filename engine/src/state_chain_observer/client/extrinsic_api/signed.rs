use std::sync::Arc;

use anyhow::{bail, Result};
use async_trait::async_trait;
use cf_primitives::AccountRole;
use futures::StreamExt;
use futures_util::FutureExt;
use sp_core::H256;
use state_chain_runtime::AccountId;
use tokio::sync::{mpsc, oneshot};
use tracing::{trace, warn};
use utilities::task_scope::{task_scope, Scope, ScopedJoinHandle, OR_CANCEL};

use crate::constants::SIGNED_EXTRINSIC_LIFETIME;

use super::{
	super::{base_rpc_api, storage_api::StorageApi, StateChainStreamApi},
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
		self.0.await.expect(OR_CANCEL)
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
		self.0.await.expect(OR_CANCEL)
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
	_task_handle: ScopedJoinHandle<()>,
}

impl SignedExtrinsicClient {
	pub async fn new<
		'a,
		BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static,
		BlockStream: StateChainStreamApi + Clone,
	>(
		scope: &Scope<'a, anyhow::Error>,
		base_rpc_client: Arc<BaseRpcClient>,
		signer: signer::PairSigner<sp_core::sr25519::Pair>,
		required_role: AccountRole,
		wait_for_required_role: bool,
		genesis_hash: H256,
		state_chain_stream: &mut BlockStream,
	) -> Result<Self> {
		const REQUEST_BUFFER: usize = 16;

		let (request_sender, mut request_receiver) = mpsc::channel(REQUEST_BUFFER);

		let account_nonce = {
			loop {
				match base_rpc_client
					.storage_map_entry::<pallet_cf_account_roles::AccountRoles<state_chain_runtime::Runtime>>(
						state_chain_stream.cache().block_hash,
						&signer.account_id,
					)
					.await?
				{
					Some(role) =>
						if required_role == AccountRole::None || required_role == role {
							break
						} else if wait_for_required_role && role == AccountRole::None {
							warn!("Your Chainflip account {} does not have an assigned account role. WAITING for the account role to be set to '{required_role:?}' at block: {}", signer.account_id, state_chain_stream.cache().block_hash);
						} else {
							bail!("Your Chainflip account {} has the wrong account role '{role:?}'. The '{required_role:?}' account role is required", signer.account_id);
						},
					None =>
						if wait_for_required_role {
							warn!("Your Chainflip account {} is not funded. Note, it may take some time for your funds to be detected. WAITING for your account to be funded at block: {}", signer.account_id, state_chain_stream.cache().block_hash);
						} else {
							bail!("Your Chainflip account {} is not funded", signer.account_id);
						},
				}

				state_chain_stream.next().await.expect(OR_CANCEL);
			}

			base_rpc_client
				.storage_map_entry::<frame_system::Account<state_chain_runtime::Runtime>>(
					state_chain_stream.cache().block_hash,
					&signer.account_id,
				)
				.await?
				.nonce
		};

		Ok(Self {
			account_id: signer.account_id.clone(),
			request_sender,
			_task_handle: scope.spawn_with_handle({
				let mut state_chain_stream = state_chain_stream.clone();

				task_scope(move |scope| async move {
					let (mut submission_watcher, mut requests) =
						submission_watcher::SubmissionWatcher::new(
							scope,
							signer,
							account_nonce,
							state_chain_stream.cache().block_hash,
							state_chain_stream.cache().block_number,
							base_rpc_client.runtime_version().await?,
							genesis_hash,
							SIGNED_EXTRINSIC_LIFETIME,
							base_rpc_client.clone(),
						);

					utilities::loop_select! {
						if let Some((call, until_in_block_sender, until_finalized_sender, strategy)) = request_receiver.recv() => {
							submission_watcher.new_request(&mut requests, call, until_in_block_sender, until_finalized_sender, strategy).await?;
						} else break Ok(()),
						let submission_details = submission_watcher.watch_for_submission_in_block() => {
							submission_watcher.on_submission_in_block(&mut requests, submission_details).await?;
						},
						if let Some((block_hash, block_header)) = state_chain_stream.next() => {
							trace!("Received state chain block: {number} ({block_hash:x?})", number = block_header.number);
							submission_watcher.on_block_finalized(
								&mut requests,
								block_hash,
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
			.await
			.expect(OR_CANCEL),
			(
				UntilInBlockFuture(until_in_block_receiver),
				UntilFinalizedFuture(until_finalized_receiver),
			),
		)
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
