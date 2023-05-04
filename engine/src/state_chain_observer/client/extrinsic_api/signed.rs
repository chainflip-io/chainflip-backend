use std::sync::Arc;

use anyhow::{bail, Result};
use async_trait::async_trait;
use cf_primitives::AccountRole;
use futures::StreamExt;
use sp_core::H256;
use state_chain_runtime::AccountId;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, trace, warn};
use utilities::task_scope::{Scope, ScopedJoinHandle, OR_CANCEL};

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
	async fn until_finalized(self) -> submission_watcher::ExtrinsicResult;
}
#[async_trait]
impl<W: UntilFinalized + Send> UntilFinalized for (state_chain_runtime::Hash, W) {
	async fn until_finalized(self) -> submission_watcher::ExtrinsicResult {
		self.1.until_finalized().await
	}
}
pub struct UntilFinalizedFuture(oneshot::Receiver<submission_watcher::ExtrinsicResult>);
#[async_trait]
impl UntilFinalized for UntilFinalizedFuture {
	async fn until_finalized(self) -> submission_watcher::ExtrinsicResult {
		self.0.await.expect(OR_CANCEL)
	}
}

// Note 'static on the generics in this trait are only required for mockall to mock it
#[async_trait]
pub trait SignedExtrinsicApi {
	type UntilFinalizedFuture: UntilFinalized + Send;

	fn account_id(&self) -> AccountId;

	async fn submit_signed_extrinsic<Call>(&self, call: Call) -> (H256, Self::UntilFinalizedFuture)
	where
		Call: Into<state_chain_runtime::RuntimeCall>
			+ Clone
			+ std::fmt::Debug
			+ Send
			+ Sync
			+ 'static;

	async fn finalize_signed_extrinsic<Call>(&self, call: Call) -> Self::UntilFinalizedFuture
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
		oneshot::Sender<submission_watcher::ExtrinsicResult>,
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
							warn!("Your Chainflip account {} is not staked. Note, if you have already staked, it may take some time for your stake to be detected. WAITING for your account to be staked at block: {}", signer.account_id, state_chain_stream.cache().block_hash);
						} else {
							bail!("Your Chainflip account {} is not staked", signer.account_id);
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

				async move {
					let (mut submission_watcher, mut requests) = submission_watcher::SubmissionWatcher::new(
						signer,
						account_nonce,
						state_chain_stream.cache().block_hash,
						state_chain_stream.cache().block_number,
						base_rpc_client.runtime_version().await?,
						genesis_hash,
						SIGNED_EXTRINSIC_LIFETIME,
						base_rpc_client.clone()
					);

					utilities::loop_select! {
						if let Some((call, result_sender, strategy)) = request_receiver.recv() => {
							submission_watcher.new_request(&mut requests, call, result_sender, strategy).await?;
						} else break Ok(()),
						if let Some((block_hash, block_header)) = state_chain_stream.next() => {
							trace!("Received state chain block: {number} ({block_hash:x?})", number = block_header.number);
							submission_watcher.on_block_finalized(
								&mut requests,
								block_hash,
							).await?;

							// TODO: Handle possibility of stuck nonce caused submissions being dropped from the mempool or broken submissions either submitted here or externally when only using submit_signed_extrinsics
							// TODO: Improve handling only submit_signed_extrinsic requests (using pending_extrinsics rpc call)
							// TODO: Use system_accountNextIndex
							{
								let mut shuffled_requests = {
									use rand::prelude::SliceRandom;
									let mut requests = requests.iter_mut().filter(|(_, request)| request.allow_resubmits).collect::<Vec<_>>();
									requests.shuffle(&mut rand::thread_rng());
									requests.into_iter()
								};

								if let Some((_, request)) = shuffled_requests.next() {
									// TODO: Detect stuck state via getting all pending extrinsics, and checking for missing extrinsics above finalized nonce
									match submission_watcher.submit_extrinsic_at_nonce(request, submission_watcher.finalized_nonce()).await? {
										Ok(_) => {
											debug!("Detected a gap in the account's submitted nonce values, pending extrinsics after this gap will not be including in blocks, unless the gap is filled. Attempting to resolve.");
											submission_watcher.anticipated_nonce = submission_watcher.finalized_nonce() + 1;
											for (_, request) in shuffled_requests {
												match submission_watcher.submit_extrinsic_at_nonce(request, submission_watcher.anticipated_nonce).await? {
													Ok(_) => {
														submission_watcher.anticipated_nonce += 1;
													},
													Err(submission_watcher::SubmissionLogicError::NonceTooLow) => break
												}
											}
										},
										Err(submission_watcher::SubmissionLogicError::NonceTooLow) => {} // Expected case, so we ignore
									}
								}
							}
						} else break Ok(()),
					}
				}
			}),
		})
	}
}

#[async_trait]
impl SignedExtrinsicApi for SignedExtrinsicClient {
	type UntilFinalizedFuture = UntilFinalizedFuture;

	fn account_id(&self) -> AccountId {
		self.account_id.clone()
	}

	async fn submit_signed_extrinsic<Call>(&self, call: Call) -> (H256, Self::UntilFinalizedFuture)
	where
		Call: Into<state_chain_runtime::RuntimeCall>
			+ Clone
			+ std::fmt::Debug
			+ Send
			+ Sync
			+ 'static,
	{
		let (result_sender, result_receiver) = oneshot::channel();
		(
			send_request(&self.request_sender, |hash_sender| {
				(
					call.into(),
					result_sender,
					submission_watcher::RequestStrategy::Submit(hash_sender),
				)
			})
			.await
			.await
			.expect(OR_CANCEL),
			UntilFinalizedFuture(result_receiver),
		)
	}

	async fn finalize_signed_extrinsic<Call>(&self, call: Call) -> Self::UntilFinalizedFuture
	where
		Call: Into<state_chain_runtime::RuntimeCall>
			+ Clone
			+ std::fmt::Debug
			+ Send
			+ Sync
			+ 'static,
	{
		UntilFinalizedFuture(
			send_request(&self.request_sender, |result_sender| {
				(call.into(), result_sender, submission_watcher::RequestStrategy::Finalize)
			})
			.await,
		)
	}
}
