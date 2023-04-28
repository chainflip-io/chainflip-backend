use std::{collections::BTreeMap, sync::Arc};

use anyhow::{bail, Result};
use async_trait::async_trait;
use cf_primitives::AccountRole;
use frame_support::dispatch::DispatchInfo;
use futures::StreamExt;
use sp_core::H256;
use state_chain_runtime::AccountId;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, trace, warn};
use utilities::task_scope::{Scope, ScopedJoinHandle, OR_CANCEL};

use crate::{
	constants::SIGNED_EXTRINSIC_LIFETIME, state_chain_observer::client::SUBSTRATE_BEHAVIOUR,
};

use super::{
	super::{base_rpc_api, storage_api::StorageApi, StateChainStreamApi},
	common::send_request,
};

pub mod signer;
mod submission_watcher;

#[derive(Error, Debug)]
pub enum FinalizationError {
	#[error("The requested transaction was not and will not be included in a finalized block")]
	NotFinalized,
	#[error(
		"The requested transaction was not (but maybe in the future) included in a finalized block"
	)]
	Unknown,
}

#[derive(Error, Debug)]
#[error("The requested transaction was included in a finalized block but its dispatch call failed: {0:?}")]
pub struct DispatchError(sp_runtime::DispatchError);

#[derive(Error, Debug)]
pub enum ExtrinsicError {
	#[error(transparent)]
	Finalization(FinalizationError),
	#[error(transparent)]
	Dispatch(DispatchError),
}

pub type ExtrinsicResult =
	Result<(H256, Vec<state_chain_runtime::RuntimeEvent>, DispatchInfo), ExtrinsicError>;

// Wrapper type to avoid await.await on submits/finalize calls being possible
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait UntilFinalized {
	async fn until_finalized(self) -> ExtrinsicResult;
}
#[async_trait]
impl<W: UntilFinalized + Send> UntilFinalized for (state_chain_runtime::Hash, W) {
	async fn until_finalized(self) -> ExtrinsicResult {
		self.1.until_finalized().await
	}
}
pub struct UntilFinalizedFuture(oneshot::Receiver<ExtrinsicResult>);
#[async_trait]
impl UntilFinalized for UntilFinalizedFuture {
	async fn until_finalized(self) -> ExtrinsicResult {
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

#[derive(Debug)]
enum Strategy {
	Submit(oneshot::Sender<H256>, oneshot::Sender<ExtrinsicResult>),
	Finalize(oneshot::Sender<ExtrinsicResult>),
}

pub struct SignedExtrinsicClient {
	account_id: AccountId,
	request_sender: mpsc::Sender<(state_chain_runtime::RuntimeCall, Strategy)>,
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
		const REQUEST_LIFETIME: u32 = 128;

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
					type RequestID = u64;
					struct Request {
						pending_submissions: usize,
						allow_resubmits: bool,
						lifetime: std::ops::RangeToInclusive<cf_primitives::BlockNumber>,
						call: state_chain_runtime::RuntimeCall,
						result_sender: oneshot::Sender<ExtrinsicResult>,
					}
					let mut next_request_id: RequestID = 0;
					let mut requests: BTreeMap<RequestID, Request> = Default::default();

					let mut submission_watcher = submission_watcher::SubmissionWatcher::new(
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
						if let Some((call, extrinsic_strategy)) = request_receiver.recv() => {
							let tx_hash = submission_watcher.submit_extrinsic(
								call.clone(),
								next_request_id,
							).await?;
							let (allow_resubmits, result_sender) = match extrinsic_strategy {
								Strategy::Submit(hash_sender, result_sender) => {
									let _result = hash_sender.send(tx_hash);
									(false, result_sender)
								},
								Strategy::Finalize(result_sender) => (true, result_sender),
							};
							requests.insert(
								next_request_id,
								Request {
									pending_submissions: 1,
									allow_resubmits,
									lifetime: ..=(state_chain_stream.cache().block_number+REQUEST_LIFETIME),
									call,
									result_sender,
								}
							);
							next_request_id += 1;
						} else break Ok(()),
						if let Some((block_hash, block_header)) = state_chain_stream.next() => {
							trace!("Received state chain block: {number} ({block_hash:x?})", number = block_header.number);
							submission_watcher.on_block_finalized(
								block_hash,
								&mut requests,
								|requests, tx_hash, _call, events, submissions| {
									// Send extrinsic request result if one of its submissions for this nonce was included in this block
									if let Some(extrinsic_request) = submissions.iter().find_map(|submission|
										requests
											.remove(&submission.identity)
									) {
										let _result = extrinsic_request.result_sender.send({
											events.iter().find_map(|event| match event {
												state_chain_runtime::RuntimeEvent::System(frame_system::Event::ExtrinsicSuccess { dispatch_info }) => {
													Some(Ok(*dispatch_info))
												},
												state_chain_runtime::RuntimeEvent::System(frame_system::Event::ExtrinsicFailed { dispatch_error, dispatch_info: _ }) => {
													Some(Err(ExtrinsicError::Dispatch(DispatchError(*dispatch_error))))
												},
												_ => None
											}).expect(SUBSTRATE_BEHAVIOUR).map(|dispatch_info| (tx_hash, events, dispatch_info))
										});
									}

									for submission in submissions {
										if let Some(request) = requests.get_mut(&submission.identity) {
											request.pending_submissions -= 1;
										}
									}
								},
								|requests, submission| {
									if let Some(request) = requests.get_mut(&submission.identity) {
										request.pending_submissions -= 1;
									}
								},
							).await?;

							let further_submissions_allowed = |request: &Request| {
								request.lifetime.contains(&(state_chain_stream.cache().block_number + 1)) && request.allow_resubmits
							};

							for (_request_id, request) in requests.drain_filter(|_request_id, request| {
								!further_submissions_allowed(request) && request.pending_submissions == 0
							}) {
								let _result = request.result_sender.send(Err(ExtrinsicError::Finalization(
									if request.pending_submissions == 0 {
										FinalizationError::NotFinalized
									} else {
										FinalizationError::Unknown
									}
								)));
							}

							for (request_id, request) in &mut requests {
								if request.pending_submissions == 0 {
									debug!("Resubmitting extrinsic as all existing submissions have expired.");
									submission_watcher.submit_extrinsic(request.call.clone(), *request_id).await?;
									request.pending_submissions += 1;
								}
							}

							// TODO: Handle possibility of stuck nonce caused submissions being dropped from the mempool or broken submissions either submitted here or externally when only using submit_signed_extrinsics
							// TODO: Improve handling only submit_signed_extrinsic requests (using pending_extrinsics rpc call)
							// TODO: Use system_accountNextIndex
							{
								let mut shuffled_requests = {
									use rand::prelude::SliceRandom;
									let mut requests = requests.iter_mut().filter(|(_, request)| further_submissions_allowed(request)).collect::<Vec<_>>();
									requests.shuffle(&mut rand::thread_rng());
									requests.into_iter()
								};

								if let Some((request_id, request)) = shuffled_requests.next() {
									// TODO: Detect stuck state via getting all pending extrinsics, and checking for missing extrinsics above finalized nonce
									match submission_watcher.submit_extrinsic_at_nonce(request.call.clone(), submission_watcher.finalized_nonce(), *request_id).await? {
										Ok(_) => {
											debug!("Detected a gap in the account's submitted nonce values, pending extrinsics after this gap will not be including in blocks, unless the gap is filled. Attempting to resolve.");
											submission_watcher.anticipated_nonce = submission_watcher.finalized_nonce();
											request.pending_submissions += 1;
											for (request_id, request) in shuffled_requests {
												match submission_watcher.submit_extrinsic_at_nonce(request.call.clone(), submission_watcher.anticipated_nonce, *request_id).await? {
													Ok(_) => {
														submission_watcher.anticipated_nonce += 1;
														request.pending_submissions += 1;
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
				(call.into(), Strategy::Submit(hash_sender, result_sender))
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
				(call.into(), Strategy::Finalize(result_sender))
			})
			.await,
		)
	}
}
