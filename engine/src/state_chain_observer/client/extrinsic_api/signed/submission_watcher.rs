use std::{collections::BTreeMap, sync::Arc};

use anyhow::{anyhow, Result};
use frame_support::{dispatch::DispatchInfo, pallet_prelude::InvalidTransaction};
use itertools::Itertools;
use sc_transaction_pool_api::TransactionStatus;
use sp_core::H256;
use sp_runtime::{traits::Hash, MultiAddress};
use thiserror::Error;
use tokio::sync::oneshot;
use tracing::{debug, warn};
use utilities::{
	future_map::FutureMap,
	task_scope::{self, Scope},
	UnendingStream,
};

use crate::state_chain_observer::client::{
	base_rpc_api,
	error_decoder::{DispatchError, ErrorDecoder},
	storage_api::StorageApi,
	SUBSTRATE_BEHAVIOUR,
};

use super::signer;

#[cfg(test)]
mod tests;

const REQUEST_LIFETIME: u32 = 128;

#[derive(Error, Debug)]
pub enum ExtrinsicError<OtherError> {
	#[error(transparent)]
	Other(OtherError),
	#[error(transparent)]
	Dispatch(DispatchError),
}

pub type ExtrinsicResult<OtherError> = Result<
	(H256, Vec<state_chain_runtime::RuntimeEvent>, state_chain_runtime::Header, DispatchInfo),
	ExtrinsicError<OtherError>,
>;

#[derive(Error, Debug)]
pub enum FinalizationError {
	#[error("The requested transaction was not and will not be included in a finalized block")]
	NotFinalized,
}

pub type FinalizationResult = ExtrinsicResult<FinalizationError>;

#[derive(Error, Debug)]
pub enum InBlockError {
	#[error("The requested transaction was not and will not be included in a block")]
	NotInBlock,
}

pub type InBlockResult = ExtrinsicResult<InBlockError>;

pub type RequestID = u64;
pub type SubmissionID = u64;

#[derive(Debug)]
pub struct Request {
	id: RequestID,
	next_submission_id: SubmissionID,
	pending_submissions: BTreeMap<SubmissionID, state_chain_runtime::Nonce>,
	strictly_one_submission: bool,
	resubmit_window: std::ops::RangeToInclusive<cf_primitives::BlockNumber>,
	call: state_chain_runtime::RuntimeCall,
	until_in_block_sender: Option<oneshot::Sender<InBlockResult>>,
	until_finalized_sender: oneshot::Sender<FinalizationResult>,
}

#[derive(Debug)]
pub enum RequestStrategy {
	StrictlyOneSubmission(oneshot::Sender<H256>),
	AllowMultipleSubmissions,
}

pub struct Submission {
	lifetime: std::ops::RangeTo<cf_primitives::BlockNumber>,
	tx_hash: H256,
	request_id: RequestID,
}

pub struct SubmissionWatcher<
	'a,
	'env,
	BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static,
> {
	scope: &'a Scope<'env, anyhow::Error>,
	submissions_by_nonce: BTreeMap<state_chain_runtime::Nonce, BTreeMap<SubmissionID, Submission>>,
	#[allow(clippy::type_complexity)]
	submission_status_futures:
		FutureMap<(RequestID, SubmissionID), task_scope::ScopedJoinHandle<Option<(H256, usize)>>>,
	signer: signer::PairSigner<sp_core::sr25519::Pair>,
	finalized_nonce: state_chain_runtime::Nonce,
	finalized_block_hash: state_chain_runtime::Hash,
	finalized_block_number: state_chain_runtime::BlockNumber,
	runtime_version: sp_version::RuntimeVersion,
	genesis_hash: state_chain_runtime::Hash,
	extrinsic_lifetime: state_chain_runtime::BlockNumber,
	base_rpc_client: Arc<BaseRpcClient>,
	error_decoder: ErrorDecoder,
}

pub enum SubmissionLogicError {
	NonceTooLow,
}

impl<'a, 'env, BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static>
	SubmissionWatcher<'a, 'env, BaseRpcClient>
{
	pub fn new(
		scope: &'a Scope<'env, anyhow::Error>,
		signer: signer::PairSigner<sp_core::sr25519::Pair>,
		finalized_nonce: state_chain_runtime::Nonce,
		finalized_block_hash: state_chain_runtime::Hash,
		finalized_block_number: state_chain_runtime::BlockNumber,
		runtime_version: sp_version::RuntimeVersion,
		genesis_hash: state_chain_runtime::Hash,
		extrinsic_lifetime: state_chain_runtime::BlockNumber,
		base_rpc_client: Arc<BaseRpcClient>,
	) -> (Self, BTreeMap<RequestID, Request>) {
		(
			Self {
				scope,
				submissions_by_nonce: Default::default(),
				submission_status_futures: Default::default(),
				signer,
				finalized_nonce,
				finalized_block_hash,
				finalized_block_number,
				runtime_version,
				genesis_hash,
				extrinsic_lifetime,
				base_rpc_client,
				error_decoder: Default::default(),
			},
			Default::default(),
		)
	}

	async fn submit_extrinsic_at_nonce(
		&mut self,
		request: &mut Request,
		nonce: state_chain_runtime::Nonce,
	) -> Result<Result<H256, SubmissionLogicError>, anyhow::Error> {
		loop {
			let (signed_extrinsic, lifetime) = self.signer.new_signed_extrinsic(
				request.call.clone(),
				&self.runtime_version,
				self.genesis_hash,
				self.finalized_block_hash,
				self.finalized_block_number,
				self.extrinsic_lifetime,
				nonce,
			);
			assert!(lifetime.contains(&(self.finalized_block_number + 1)));

			let tx_hash: H256 = {
				use sp_core::{blake2_256, Encode};
				let encoded = signed_extrinsic.encode();
				blake2_256(&encoded).into()
			};

			match self.base_rpc_client.submit_and_watch_extrinsic(signed_extrinsic).await {
				Ok(mut transaction_status_stream) => {
					request.pending_submissions.insert(request.next_submission_id, nonce);
					self.submissions_by_nonce.entry(nonce).or_default().insert(
						request.next_submission_id,
						Submission { lifetime, tx_hash, request_id: request.id },
					);
					self.submission_status_futures.insert(
						(request.id, request.next_submission_id),
						self.scope.spawn_with_handle(async move {
							while let Some(status) = transaction_status_stream.next().await {
								if let TransactionStatus::InBlock((block_hash, extrinsic_index)) =
									status?
								{
									return Ok(Some((block_hash, extrinsic_index)))
								}
							}

							Ok(None)
						}),
					);
					request.next_submission_id += 1;
					break Ok(Ok(tx_hash))
				},
				Err(rpc_err) => {
					fn invalid_err_obj(
						invalid_reason: InvalidTransaction,
					) -> jsonrpsee::types::ErrorObjectOwned {
						jsonrpsee::types::ErrorObject::owned(
							1010,
							"Invalid Transaction",
							Some(<&'static str>::from(invalid_reason)),
						)
					}

					match rpc_err {
						// This occurs when a transaction with the same nonce is in the
						// transaction pool (and the priority is <= priority of that
						// existing tx)
						jsonrpsee::core::Error::Call(
							jsonrpsee::types::error::CallError::Custom(ref obj),
						) if obj.code() == 1014 => {
							debug!("Failed as transaction with same nonce found in transaction pool: {obj:?}");
							break Ok(Err(SubmissionLogicError::NonceTooLow))
						},
						// This occurs when the nonce has already been *consumed* i.e a
						// transaction with that nonce is in a block
						jsonrpsee::core::Error::Call(
							jsonrpsee::types::error::CallError::Custom(ref obj),
						) if obj == &invalid_err_obj(InvalidTransaction::Stale) => {
							debug!("Failed as the transaction is stale: {obj:?}");
							break Ok(Err(SubmissionLogicError::NonceTooLow))
						},
						jsonrpsee::core::Error::Call(
							jsonrpsee::types::error::CallError::Custom(ref obj),
						) if obj == &invalid_err_obj(InvalidTransaction::BadProof) => {
							warn!("Failed due to a bad proof: {obj:?}. Refetching the runtime version.");

							// TODO: Check if hash and block number should also be updated
							// here

							let new_runtime_version =
								self.base_rpc_client.runtime_version().await?;
							if new_runtime_version == self.runtime_version {
								// break, as the error is now very unlikely to be solved by
								// fetching again
								return Err(anyhow!("Fetched RuntimeVersion of {:?} is the same as the previous RuntimeVersion. This is not expected.", self.runtime_version))
							}

							self.runtime_version = new_runtime_version;
						},
						err => break Err(err.into()),
					}
				},
			}
		}
	}

	async fn submit_extrinsic(&mut self, request: &mut Request) -> Result<H256, anyhow::Error> {
		Ok(loop {
			let nonce =
				self.base_rpc_client.next_account_nonce(self.signer.account_id.clone()).await?;
			match self.submit_extrinsic_at_nonce(request, nonce).await? {
				Ok(tx_hash) => break tx_hash,
				Err(SubmissionLogicError::NonceTooLow) => {},
			}
		})
	}

	pub async fn new_request(
		&mut self,
		requests: &mut BTreeMap<RequestID, Request>,
		call: state_chain_runtime::RuntimeCall,
		until_in_block_sender: oneshot::Sender<InBlockResult>,
		until_finalized_sender: oneshot::Sender<FinalizationResult>,
		strategy: RequestStrategy,
	) -> Result<(), anyhow::Error> {
		let id = requests.keys().next_back().map(|id| id + 1).unwrap_or(0);
		let request = requests
			.try_insert(
				id,
				Request {
					id,
					next_submission_id: 0,
					pending_submissions: Default::default(),
					strictly_one_submission: matches!(
						strategy,
						RequestStrategy::StrictlyOneSubmission(_)
					),
					resubmit_window: ..=(self.finalized_block_number + 1 + REQUEST_LIFETIME),
					call,
					until_in_block_sender: Some(until_in_block_sender),
					until_finalized_sender,
				},
			)
			.unwrap();
		let tx_hash: H256 = self.submit_extrinsic(request).await?;
		if let RequestStrategy::StrictlyOneSubmission(hash_sender) = strategy {
			let _result = hash_sender.send(tx_hash);
		};
		Ok(())
	}

	fn decide_extrinsic_success<OtherError>(
		&self,
		tx_hash: H256,
		extrinsic_events: Vec<state_chain_runtime::RuntimeEvent>,
		header: state_chain_runtime::Header,
	) -> ExtrinsicResult<OtherError> {
		extrinsic_events
			.iter()
			.find_map(|event| match event {
				state_chain_runtime::RuntimeEvent::System(
					frame_system::Event::ExtrinsicSuccess { dispatch_info },
				) => Some(Ok(*dispatch_info)),
				state_chain_runtime::RuntimeEvent::System(
					frame_system::Event::ExtrinsicFailed { dispatch_error, dispatch_info: _ },
				) => Some(Err(ExtrinsicError::Dispatch(
					self.error_decoder.decode_dispatch_error(*dispatch_error),
				))),
				_ => None,
			})
			.expect(SUBSTRATE_BEHAVIOUR)
			.map(|dispatch_info| (tx_hash, extrinsic_events, header, dispatch_info))
	}

	pub async fn watch_for_submission_in_block(
		&mut self,
	) -> (RequestID, SubmissionID, H256, usize) {
		loop {
			if let ((request_id, submission_id), Some((tx_hash, extrinsic_index))) =
				self.submission_status_futures.next_or_pending().await
			{
				return (request_id, submission_id, tx_hash, extrinsic_index)
			}
		}
	}

	pub async fn on_submission_in_block(
		&mut self,
		requests: &mut BTreeMap<RequestID, Request>,
		(request_id, _submission_id, block_hash, extrinsic_index): (
			RequestID,
			SubmissionID,
			H256,
			usize,
		),
	) -> Result<(), anyhow::Error> {
		if let Some(block) = self.base_rpc_client.block(block_hash).await? {
			let extrinsic = block.block.extrinsics.get(extrinsic_index).expect(SUBSTRATE_BEHAVIOUR);

			let tx_hash =
				<state_chain_runtime::Runtime as frame_system::Config>::Hashing::hash_of(extrinsic);

			let extrinsic_events = self
				.base_rpc_client
				.storage_value::<frame_system::Events<state_chain_runtime::Runtime>>(block_hash)
				.await?
				.into_iter()
				.filter_map(|event_record| match *event_record {
					frame_system::EventRecord {
						phase: frame_system::Phase::ApplyExtrinsic(index),
						event,
						..
					} if index as usize == extrinsic_index => Some(event),
					_ => None,
				})
				.collect::<Vec<_>>();

			if let Some(request) = requests.get_mut(&request_id) {
				if let Some(until_in_block_sender) = request.until_in_block_sender.take() {
					let _result = until_in_block_sender.send(self.decide_extrinsic_success(
						tx_hash,
						extrinsic_events,
						block.block.header,
					));
				}
			}
		}

		Ok(())
	}

	pub async fn on_block_finalized(
		&mut self,
		requests: &mut BTreeMap<RequestID, Request>,
		block_hash: H256,
	) -> Result<(), anyhow::Error> {
		let block = self.base_rpc_client.block(block_hash).await?.expect(SUBSTRATE_BEHAVIOUR).block;
		// TODO: Move this out into BlockProducer
		let events = self
			.base_rpc_client
			.storage_value::<frame_system::Events<state_chain_runtime::Runtime>>(block_hash)
			.await?;

		assert_eq!(block.header.number, self.finalized_block_number + 1, "{SUBSTRATE_BEHAVIOUR}");

		let nonce = self
			.base_rpc_client
			.storage_map_entry::<frame_system::Account<state_chain_runtime::Runtime>>(
				block_hash,
				&self.signer.account_id,
			)
			.await?
			.nonce;

		if nonce < self.finalized_nonce {
			Err(anyhow!("Extrinsic signer's account was reaped"))
		} else {
			// TODO: Get hash and number from the RPC and use std::cmp::max() here
			self.finalized_block_number = block.header.number;
			self.finalized_block_hash = block_hash;

			self.finalized_nonce = nonce;

			for (extrinsic_index, extrinsic_events) in events
				.into_iter()
				.filter_map(|event_record| match *event_record {
					frame_system::EventRecord {
						phase: frame_system::Phase::ApplyExtrinsic(extrinsic_index),
						event,
						..
					} => Some((extrinsic_index, event)),
					_ => None,
				})
				.sorted_by_key(|(extrinsic_index, _)| *extrinsic_index)
				.group_by(|(extrinsic_index, _)| *extrinsic_index)
				.into_iter()
				.map(|(extrinsic_index, extrinsic_events)| {
					(extrinsic_index, extrinsic_events.map(|(_extrinsics_index, event)| event))
				}) {
				let extrinsic = &block.extrinsics[extrinsic_index as usize];
				// TODO: Assumption needs checking
				if let Some(submissions) = extrinsic.signature.as_ref().and_then(
					|(address, _, (.., frame_system::CheckNonce(nonce), _, _))| {
						(*address == MultiAddress::Id(self.signer.account_id.clone()))
							.then_some(())
							.and_then(|_| self.submissions_by_nonce.remove(nonce))
					},
				) {
					let tx_hash =
						<state_chain_runtime::Runtime as frame_system::Config>::Hashing::hash_of(
							extrinsic,
						);

					let mut not_found_matching_submission = Some(extrinsic_events);

					for (submission_id, submission) in submissions {
						if let Some(request) = requests.get_mut(&submission.request_id) {
							request.pending_submissions.remove(&submission_id).unwrap();
							self.submission_status_futures.remove((request.id, submission_id));
						}

						// Note: It is technically possible for a hash collision to
						// occur, but it is so unlikely it is effectively
						// impossible. If it where to occur this code would not
						// notice the extrinsic was not actually the requested one,
						// but otherwise would continue to work.
						if let Some((extrinsic_events, matching_request)) =
							(not_found_matching_submission.is_some() &&
								submission.tx_hash == tx_hash)
								.then_some(())
								.and_then(|_| requests.remove(&submission.request_id))
								.map(|request| {
									(not_found_matching_submission.take().unwrap(), request)
								}) {
							let extrinsic_events = extrinsic_events.collect::<Vec<_>>();
							let result = self.decide_extrinsic_success(
								tx_hash,
								extrinsic_events,
								block.header.clone(),
							);
							if let Some(until_in_block_sender) =
								matching_request.until_in_block_sender
							{
								let _result = until_in_block_sender.send(
									result.as_ref().map(Clone::clone).map_err(
										|error| match error {
											ExtrinsicError::Dispatch(dispatch_error) =>
												ExtrinsicError::Dispatch(dispatch_error.clone()),
											ExtrinsicError::Other(
												FinalizationError::NotFinalized,
											) => ExtrinsicError::Other(InBlockError::NotInBlock),
										},
									),
								);
							}
							let _result = matching_request.until_finalized_sender.send(result);
						}
					}
				}
			}

			self.submissions_by_nonce.retain(|nonce, submissions| {
				assert!(self.finalized_nonce <= *nonce, "{SUBSTRATE_BEHAVIOUR}");

				submissions.retain(|submission_id, submission| {
					let alive = submission.lifetime.contains(&(block.header.number + 1));

					if !alive {
						if let Some(request) = requests.get_mut(&submission.request_id) {
							request.pending_submissions.remove(submission_id).unwrap();
							self.submission_status_futures.remove((request.id, *submission_id));
						}
					}

					alive
				});

				!submissions.is_empty()
			});

			for (_request_id, request) in requests.extract_if(|_request_id, request| {
				request.pending_submissions.is_empty() &&
					(!request.resubmit_window.contains(&(block.header.number + 1)) ||
						request.strictly_one_submission)
			}) {
				let _result = request
					.until_finalized_sender
					.send(Err(ExtrinsicError::Other(FinalizationError::NotFinalized)));
			}

			// Has to be a separate loop from the above due to not being able to await inside
			// extract_if
			for (_request_id, request) in requests.iter_mut() {
				if request.pending_submissions.is_empty() {
					debug!("Resubmitting extrinsic as all existing submissions have expired.");
					self.submit_extrinsic(request).await?;
				}
			}

			Ok(())
		}
	}
}
