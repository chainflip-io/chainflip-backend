use std::{
	collections::{BTreeMap, VecDeque},
	sync::Arc,
};

use anyhow::{anyhow, Result};
use codec::{Decode, Encode};
use frame_support::{dispatch::DispatchInfo, pallet_prelude::InvalidTransaction};
use itertools::Itertools;
use sc_transaction_pool_api::TransactionStatus;
use sp_core::H256;
use sp_runtime::{
	traits::Hash, transaction_validity::TransactionValidityError, ApplyExtrinsicResult,
	MultiAddress,
};
use state_chain_runtime::{BlockNumber, Nonce, UncheckedExtrinsic};
use thiserror::Error;
use tokio::sync::oneshot;
use tracing::{debug, info, warn};
use utilities::{
	future_map::FutureMap,
	task_scope::{self, Scope},
	UnendingStream,
};

use crate::state_chain_observer::client::{
	base_rpc_api,
	error_decoder::{DispatchError, ErrorDecoder},
	extrinsic_api::common::invalid_err_obj,
	storage_api::{CheckBlockCompatibility, StorageApi},
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

#[derive(Error, Debug)]
pub enum DryRunError {
	#[error(transparent)]
	RpcCallError(#[from] jsonrpsee::core::Error),
	#[error("Unable to decode dry_run RPC result: {0}")]
	CannotDecodeReply(#[from] codec::Error),
	#[error("The transaction is invalid: {0}")]
	InvalidTransaction(#[from] TransactionValidityError),
	#[error("The transaction failed: {0}")]
	Dispatch(#[from] DispatchError),
}

pub type ExtrinsicDetails =
	(H256, Vec<state_chain_runtime::RuntimeEvent>, state_chain_runtime::Header, DispatchInfo);

pub type ExtrinsicResult<OtherError> = Result<ExtrinsicDetails, ExtrinsicError<OtherError>>;

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
	pending_submissions: BTreeMap<SubmissionID, Nonce>,
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
	submissions_by_nonce: BTreeMap<Nonce, BTreeMap<SubmissionID, Submission>>,
	#[allow(clippy::type_complexity)]
	submission_status_futures:
		FutureMap<(RequestID, SubmissionID), task_scope::ScopedJoinHandle<Option<(H256, H256)>>>,
	signer: signer::PairSigner<sp_core::sr25519::Pair>,
	finalized_nonce: Nonce,
	finalized_block_hash: state_chain_runtime::Hash,
	finalized_block_number: BlockNumber,
	runtime_version: sp_version::RuntimeVersion,
	genesis_hash: state_chain_runtime::Hash,
	extrinsic_lifetime: BlockNumber,
	#[allow(clippy::type_complexity)]
	block_cache: VecDeque<(
		state_chain_runtime::Hash,
		Option<(
			state_chain_runtime::Header,
			Vec<UncheckedExtrinsic>,
			Vec<Box<frame_system::EventRecord<state_chain_runtime::RuntimeEvent, H256>>>,
		)>,
	)>,
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
		finalized_nonce: Nonce,
		finalized_block_hash: state_chain_runtime::Hash,
		finalized_block_number: BlockNumber,
		runtime_version: sp_version::RuntimeVersion,
		genesis_hash: state_chain_runtime::Hash,
		extrinsic_lifetime: BlockNumber,
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
				block_cache: Default::default(),
				base_rpc_client,
				error_decoder: Default::default(),
			},
			// Return an empty requests map. This is done so that initial state of the requests
			// matches the submission watchers state. The requests must be stored outside of
			// the watcher so it can be manipulated by it's parent while holding a mut reference
			// to the watcher.
			Default::default(),
		)
	}

	fn build_and_sign_extrinsic(
		&self,
		call: state_chain_runtime::RuntimeCall,
		nonce: Nonce,
	) -> state_chain_runtime::UncheckedExtrinsic {
		self.signer
			.new_signed_extrinsic(
				call.clone(),
				&self.runtime_version,
				self.genesis_hash,
				self.finalized_block_hash,
				self.finalized_block_number,
				self.extrinsic_lifetime,
				nonce,
			)
			.0
	}

	async fn submit_extrinsic_at_nonce(
		&mut self,
		request: &mut Request,
		nonce: Nonce,
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
				let encoded = signed_extrinsic.encode();
				sp_core::blake2_256(&encoded).into()
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
								// NOTE: Currently the _extrinsic_index returned by substrate
								// through the subscription is wrong and is always 0
								if let TransactionStatus::InBlock((block_hash, _extrinsic_index)) =
									status?
								{
									return Ok(Some((block_hash, tx_hash)))
								}
							}

							Ok(None)
						}),
					);
					info!(target: "state_chain_client", request_id = request.id, submission_id = request.next_submission_id, "Submission succeeded");
					request.next_submission_id += 1;
					break Ok(Ok(tx_hash))
				},
				Err(rpc_err) => {
					match rpc_err {
						// This occurs when a transaction with the same nonce is in the
						// transaction pool (and the priority is <= priority of that
						// existing tx)
						jsonrpsee::core::Error::Call(
							jsonrpsee::types::error::CallError::Custom(ref obj),
						) if obj.code() == 1014 => {
							debug!(target: "state_chain_client", request_id = request.id, "Submission failed as transaction with same nonce found in transaction pool: {obj:?}");
							break Ok(Err(SubmissionLogicError::NonceTooLow))
						},
						// This occurs when the nonce has already been *consumed* i.e a
						// transaction with that nonce is in a block
						jsonrpsee::core::Error::Call(
							jsonrpsee::types::error::CallError::Custom(ref obj),
						) if obj == &invalid_err_obj(InvalidTransaction::Stale) => {
							debug!(target: "state_chain_client", request_id = request.id, "Submission failed as the transaction is stale: {obj:?}");
							break Ok(Err(SubmissionLogicError::NonceTooLow))
						},
						jsonrpsee::core::Error::Call(
							jsonrpsee::types::error::CallError::Custom(ref obj),
						) if obj == &invalid_err_obj(InvalidTransaction::BadProof) => {
							warn!(target: "state_chain_client", request_id = request.id, "Submission failed due to a bad proof: {obj:?}. Refetching the runtime version.");

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

	pub async fn dry_run_extrinsic(
		&mut self,
		call: state_chain_runtime::RuntimeCall,
	) -> Result<(), DryRunError> {
		// Use the nonce from the latest unfinalized block.
		let hash = self.base_rpc_client.latest_unfinalized_block_hash().await?;
		let nonce = self
			.base_rpc_client
			.storage_map_entry::<frame_system::Account<state_chain_runtime::Runtime>>(
				hash,
				&self.signer.account_id,
			)
			.await?
			.nonce;
		let uxt = self.build_and_sign_extrinsic(call.clone(), nonce);
		let result_bytes = self.base_rpc_client.dry_run(Encode::encode(&uxt).into(), None).await?;
		let dry_run_result: ApplyExtrinsicResult = Decode::decode(&mut &*result_bytes)?;

		debug!(target: "state_chain_client", "Dry run completed. \nCall:{:?} \nResult: {:?}", call, &dry_run_result);

		Ok(dry_run_result?.map_err(|e| self.error_decoder.decode_dispatch_error(e))?)
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
		info!(target: "state_chain_client", request_id = request.id, "New request: {:?}", request.call);
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
		// We expect to find a Success or Failed event, grab the dispatch info and send
		// it with the events
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

	pub async fn watch_for_submission_in_block(&mut self) -> (RequestID, SubmissionID, H256, H256) {
		loop {
			if let ((request_id, submission_id), Some((block_hash, tx_hash))) =
				self.submission_status_futures.next_or_pending().await
			{
				return (request_id, submission_id, block_hash, tx_hash)
			}
		}
	}

	pub async fn on_submission_in_block(
		&mut self,
		requests: &mut BTreeMap<RequestID, Request>,
		(request_id, submission_id, block_hash, tx_hash): (RequestID, SubmissionID, H256, H256),
	) -> Result<(), anyhow::Error> {
		if let Some((header, extrinsics, events)) = if let Some((
			_,
			cached_compatible_block_details,
		)) = self
			.block_cache
			.iter()
			.find(|(cached_block_hash, ..)| block_hash == *cached_block_hash)
		{
			cached_compatible_block_details.as_ref()
		} else if let Some(block) = self.base_rpc_client.block(block_hash).await? {
			if self.block_cache.len() >= 4 {
				self.block_cache.pop_front();
			}
			self.block_cache.push_back((
				block_hash,
				if self.base_rpc_client.check_block_compatibility(block_hash).await?.is_ok() {
					Some((
						block.block.header,
						block.block.extrinsics,
						self.base_rpc_client
							.storage_value::<frame_system::Events<state_chain_runtime::Runtime>>(
								block_hash,
							)
							.await?,
					))
				} else {
					None
				},
			));

			self.block_cache.back().unwrap().1.as_ref()
		} else {
			warn!(target: "state_chain_client", request_id = request_id, submission_id = submission_id, "Block not found with hash {block_hash:?}");
			None
		} {
			let (extrinsic_index, _extrinsic) = extrinsics
				.iter()
				.enumerate()
				.find(|(_extrinsic_index, extrinsic)| {
					tx_hash ==
						<state_chain_runtime::Runtime as frame_system::Config>::Hashing::hash_of(
							extrinsic,
						)
				})
				.expect(SUBSTRATE_BEHAVIOUR);

			let extrinsic_events = events
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

			if let Some(request) = requests.get_mut(&request_id) {
				info!(target: "state_chain_client", request_id = request_id, submission_id = submission_id, "Request found in block with hash {block_hash:?}, tx_hash {tx_hash:?}, and extrinsic index {extrinsic_index}.");
				let until_in_block_sender = request.until_in_block_sender.take().unwrap();
				let _result = until_in_block_sender.send(self.decide_extrinsic_success(
					tx_hash,
					extrinsic_events,
					header.clone(),
				));
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

		// Get our account nonce and compare it to the finalized nonce
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
			// Update the finalized data
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

				// Find any submissions that are for the nonce of the extrinsic
				if let Some(submissions) = extrinsic.signature.as_ref().and_then(
					|(address, _, (.., frame_system::CheckNonce(nonce), _, _))| {
						// We only care about the extrinsic if it is from our account
						(*address == MultiAddress::Id(self.signer.account_id.clone()))
							.then_some(())
							.and_then(|_| self.submissions_by_nonce.remove(nonce))
					},
				) {
					let tx_hash =
						<state_chain_runtime::Runtime as frame_system::Config>::Hashing::hash_of(
							extrinsic,
						);

					let mut optional_extrinsic_events = Some(extrinsic_events);

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
							(optional_extrinsic_events.is_some() && submission.tx_hash == tx_hash)
								.then_some(())
								.and_then(|_| requests.remove(&submission.request_id))
								.map(|request| {
									// If its the right hash, take the events and the request
									(optional_extrinsic_events.take().unwrap(), request)
								}) {
							let extrinsic_events = extrinsic_events.collect::<Vec<_>>();
							let result = self.decide_extrinsic_success(
								tx_hash,
								extrinsic_events,
								block.header.clone(),
							);
							info!(target: "state_chain_client", request_id = matching_request.id, submission_id = submission_id, "Request found in finalized block with hash {block_hash:?}, tx_hash {tx_hash:?}, and extrinsic index {extrinsic_index}.");
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

			// Remove any submissions that have expired
			self.submissions_by_nonce.retain(|nonce, submissions| {
				assert!(self.finalized_nonce <= *nonce, "{SUBSTRATE_BEHAVIOUR}");

				submissions.retain(|submission_id, submission| {
					let alive = submission.lifetime.contains(&(block.header.number + 1));

					if !alive {
						info!(target: "state_chain_client", request_id = submission.request_id, submission_id = submission_id, "Submission has timed out.");
						if let Some(request) = requests.get_mut(&submission.request_id) {
							request.pending_submissions.remove(submission_id).unwrap();
						}
						self.submission_status_futures.remove((submission.request_id, *submission_id));
					}

					alive
				});

				!submissions.is_empty()
			});

			// Remove any requests that have all their submission have expired and whose
			// resubmission window has past.
			for (_request_id, request) in requests.extract_if(|_request_id, request| {
				request.pending_submissions.is_empty() &&
					(!request.resubmit_window.contains(&(block.header.number + 1)) ||
						request.strictly_one_submission)
			}) {
				info!(target: "state_chain_client", request_id = request.id, "Request has timed out.");
				if let Some(until_in_block_sender) = request.until_in_block_sender {
					let _result = until_in_block_sender
						.send(Err(ExtrinsicError::Other(InBlockError::NotInBlock)));
				}
				let _result = request
					.until_finalized_sender
					.send(Err(ExtrinsicError::Other(FinalizationError::NotFinalized)));
			}
			// Resubmit any expired requests that have no unexpired submission.
			// This has to be a separate loop from the above due to not being able to await inside
			// extract_if
			for (_request_id, request) in requests.iter_mut() {
				if request.pending_submissions.is_empty() {
					info!("Resubmitting extrinsic as all existing submissions have expired.");
					self.submit_extrinsic(request).await?;
				}
			}

			Ok(())
		}
	}
}
