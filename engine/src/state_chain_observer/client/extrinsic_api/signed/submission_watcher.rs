use std::{
	collections::{BTreeMap, HashMap},
	sync::Arc,
};

use anyhow::{anyhow, bail, Result};
use cf_primitives::BlockNumber;
use frame_support::{dispatch::DispatchInfo, pallet_prelude::InvalidTransaction};
use itertools::Itertools;
use sp_core::H256;
use sp_runtime::{traits::Hash, MultiAddress};
use state_chain_runtime::Index as Nonce;
use thiserror::Error;
use tokio::sync::oneshot;
use tracing::{debug, warn};

use crate::state_chain_observer::client::{
	base_rpc_api, storage_api::StorageApi, SUBSTRATE_BEHAVIOUR,
};

use super::signer;

#[cfg(test)]
mod tests;

const REQUEST_LIFETIME: u32 = 128;

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

pub type ExtrinsicResult = Result<
	(H256, Vec<state_chain_runtime::RuntimeEvent>, state_chain_runtime::Header, DispatchInfo),
	ExtrinsicError,
>;

pub type RequestID = u64;

#[derive(Debug)]
pub struct Request {
	id: RequestID,
	pending_submissions: usize,
	pub allow_resubmits: bool,
	lifetime: std::ops::RangeToInclusive<BlockNumber>,
	call: state_chain_runtime::RuntimeCall,
	result_sender: oneshot::Sender<ExtrinsicResult>,
}

#[derive(Debug)]
pub enum RequestStrategy {
	Submit(oneshot::Sender<H256>),
	Finalize,
}

#[derive(Debug)]
pub struct Submission {
	lifetime: std::ops::RangeTo<BlockNumber>,
	tx_hash: H256,
	request_id: RequestID,
}

pub struct SubmissionWatcher<BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static> {
	submissions_by_nonce: BTreeMap<Nonce, Vec<Submission>>,
	// The locally tracked nonce used to submit multiple extrinsics in a single block. A new
	// request will be submitted at this nonce.
	pub anticipated_nonce: Nonce,
	signer: signer::PairSigner<sp_core::sr25519::Pair>,
	// Our account nonce at the time of the last finalized block, ie. the nonce of the next
	// extrinsic that will be accepted.
	finalized_nonce: Nonce,
	finalized_block_hash: state_chain_runtime::Hash,
	finalized_block_number: BlockNumber,
	runtime_version: sp_version::RuntimeVersion,
	genesis_hash: state_chain_runtime::Hash,
	extrinsic_lifetime: BlockNumber,
	base_rpc_client: Arc<BaseRpcClient>,
}

pub enum SubmissionLogicError {
	NonceTooLow,
}

impl<BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static>
	SubmissionWatcher<BaseRpcClient>
{
	pub fn new(
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
				submissions_by_nonce: Default::default(),
				anticipated_nonce: finalized_nonce,
				signer,
				finalized_nonce,
				finalized_block_hash,
				finalized_block_number,
				runtime_version,
				genesis_hash,
				extrinsic_lifetime,
				base_rpc_client,
			},
			// Return an empty requests map. This is done so that initial state of the requests
			// matches the submission watchers state. The requests must be stored outside of
			// the watcher so it can be manipulated by it's parent while holding a mut reference
			// to the watcher.
			Default::default(),
		)
	}

	pub fn finalized_nonce(&self) -> Nonce {
		self.finalized_nonce
	}

	pub async fn submit_extrinsic_at_nonce(
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

			match self.base_rpc_client.submit_extrinsic(signed_extrinsic).await {
				Ok(tx_hash) => {
					request.pending_submissions += 1;
					self.submissions_by_nonce.entry(nonce).or_default().push(Submission {
						lifetime,
						tx_hash,
						request_id: request.id,
					});
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

	pub async fn submit_extrinsic(&mut self, request: &mut Request) -> Result<H256, anyhow::Error> {
		Ok(loop {
			match self.submit_extrinsic_at_nonce(request, self.anticipated_nonce).await? {
				Ok(tx_hash) => {
					self.anticipated_nonce += 1;
					break tx_hash
				},
				Err(SubmissionLogicError::NonceTooLow) => {
					self.anticipated_nonce += 1;
				},
			}
		})
	}

	pub async fn new_request(
		&mut self,
		requests: &mut BTreeMap<RequestID, Request>,
		call: state_chain_runtime::RuntimeCall,
		result_sender: oneshot::Sender<ExtrinsicResult>,
		strategy: RequestStrategy,
	) -> Result<(), anyhow::Error> {
		let id = requests.keys().next_back().map(|id| id + 1).unwrap_or(0);
		let request = requests
			.try_insert(
				id,
				Request {
					id,
					pending_submissions: 0,
					allow_resubmits: match &strategy {
						RequestStrategy::Submit(_) => false,
						RequestStrategy::Finalize => true,
					},
					lifetime: ..=(self.finalized_block_number + 1 + REQUEST_LIFETIME),
					call,
					result_sender,
				},
			)
			.unwrap();
		let tx_hash: H256 = self.submit_extrinsic(request).await?;
		if let RequestStrategy::Submit(hash_sender) = strategy {
			let _result = hash_sender.send(tx_hash);
		};
		Ok(())
	}

	pub async fn on_block_finalized(
		&mut self,
		requests: &mut BTreeMap<RequestID, Request>,
		block_hash: H256,
	) -> Result<()> {
		let block = self.base_rpc_client.block(block_hash).await?.expect(SUBSTRATE_BEHAVIOUR).block;
		// TODO: Move this out into BlockProducer
		let events = self
			.base_rpc_client
			.storage_value::<frame_system::Events<state_chain_runtime::Runtime>>(block_hash)
			.await?;

		self.update_finalized_data(block_hash, block.header.number).await?;

		// Find any events that have an extrinsic index
		let event_vec: Vec<_> = events
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
			.collect();

		// Group the events by extrinsic index
		let events_for_extrinsic: HashMap<u32, Vec<_>> = event_vec
			.into_iter()
			.group_by(|(extrinsic_index, _)| *extrinsic_index)
			.into_iter()
			.map(|(extrinsic_index, extrinsic_events)| {
				(extrinsic_index, extrinsic_events.map(|(_, event)| event).collect())
			})
			.collect();

		// Process the extrinsic events
		events_for_extrinsic
			.into_iter()
			.for_each(|(extrinsic_index, extrinsic_events)| {
				let extrinsic = &block.extrinsics[extrinsic_index as usize];
				self.find_submission_and_process(extrinsic, extrinsic_events, requests, &block);
			});

		self.cleanup_submissions(block.header.number, requests);

		self.cleanup_requests(block.header.number, requests).await?;

		Ok(())
	}

	async fn update_finalized_data(
		&mut self,
		block_hash: H256,
		block_number: BlockNumber,
	) -> Result<()> {
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
			bail!("Extrinsic signer's account was reaped");
		}

		// Update the finalized data
		self.finalized_block_number = block_number;
		self.finalized_block_hash = block_hash;
		self.finalized_nonce = nonce;

		// TODO: Get hash and number from the RPC and use std::cmp::max() here

		// Fast forward the anticipated nonce to the finalized nonce if it is behind
		self.anticipated_nonce = Nonce::max(self.anticipated_nonce, nonce);

		Ok(())
	}

	/// Find any submissions that match the extrinsic and process them
	fn find_submission_and_process(
		&mut self,
		extrinsic: &state_chain_runtime::UncheckedExtrinsic,
		extrinsic_events: Vec<state_chain_runtime::RuntimeEvent>,
		requests: &mut BTreeMap<RequestID, Request>,
		block: &state_chain_runtime::Block,
	) {
		// TODO: Assumption needs checking
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
				<state_chain_runtime::Runtime as frame_system::Config>::Hashing::hash_of(extrinsic);

			// Put the events in an option so that it can be taken if a matching submission is found
			// in the loop below, without having to clone the events.
			let mut optional_extrinsic_events = Some(extrinsic_events);

			for submission in submissions {
				if let Some(request) = requests.get_mut(&submission.request_id) {
					request.pending_submissions -= 1;
				}

				// Note: It is technically possible for a hash collision to occur, but it is so
				// unlikely it is effectively impossible. If it where to occur this code would not
				// notice the extrinsic was not actually the requested one, but otherwise would
				// continue to work.
				if let Some((extrinsic_events, matching_request)) =
					// If its the right hash, take the events and the request
					(optional_extrinsic_events.is_some() && submission.tx_hash == tx_hash)
							.then_some(())
							.and_then(|_| requests.remove(&submission.request_id))
							.map(|request| (optional_extrinsic_events.take().unwrap(), request))
				{
					// We expect to find a Success or Failed event, grab the dispatch info and send
					// it with the events, completing the request.
					let _result = matching_request.result_sender.send({
						extrinsic_events
							.iter()
							.find_map(|event| match event {
								state_chain_runtime::RuntimeEvent::System(
									frame_system::Event::ExtrinsicSuccess { dispatch_info },
								) => Some(Ok(*dispatch_info)),
								state_chain_runtime::RuntimeEvent::System(
									frame_system::Event::ExtrinsicFailed {
										dispatch_error,
										dispatch_info: _,
									},
								) => Some(Err(ExtrinsicError::Dispatch(DispatchError(
									*dispatch_error,
								)))),
								_ => None,
							})
							.expect(SUBSTRATE_BEHAVIOUR)
							.map(|dispatch_info| {
								(tx_hash, extrinsic_events, block.header.clone(), dispatch_info)
							})
					});
				}
			}
		}
	}

	/// Remove any submissions that have expired
	fn cleanup_submissions(
		&mut self,
		block_number: BlockNumber,
		requests: &mut BTreeMap<RequestID, Request>,
	) {
		self.submissions_by_nonce.retain(|nonce, submissions| {
			assert!(self.finalized_nonce <= *nonce, "{SUBSTRATE_BEHAVIOUR}");

			submissions.retain(|submission| {
				let alive = submission.lifetime.contains(&(block_number + 1));

				if !alive {
					if let Some(request) = requests.get_mut(&submission.request_id) {
						request.pending_submissions -= 1;
					}
				}

				alive
			});

			!submissions.is_empty()
		});
	}

	/// Remove or resubmit any requests that have expired
	async fn cleanup_requests(
		&mut self,
		block_number: BlockNumber,
		requests: &mut BTreeMap<RequestID, Request>,
	) -> Result<()> {
		// Remove any expired requests that don't allow resubmits
		for (_request_id, request) in requests.drain_filter(|_request_id, request| {
			!request.lifetime.contains(&(block_number + 1)) ||
				!request.allow_resubmits && request.pending_submissions == 0
		}) {
			let _result = request.result_sender.send(Err(ExtrinsicError::Finalization(
				if request.pending_submissions == 0 {
					FinalizationError::NotFinalized
				} else {
					FinalizationError::Unknown
				},
			)));
		}

		// Resubmit any expired requests that are left
		for (_request_id, request) in requests.iter_mut() {
			if request.pending_submissions == 0 {
				debug!("Resubmitting extrinsic as all existing submissions have expired.");
				self.submit_extrinsic(request).await?;
			}
		}

		Ok(())
	}
}
