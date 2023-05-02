use std::{collections::BTreeMap, sync::Arc};

use anyhow::{anyhow, Result};
use frame_support::{dispatch::DispatchInfo, pallet_prelude::InvalidTransaction};
use itertools::Itertools;
use sp_core::H256;
use sp_runtime::{traits::Hash, MultiAddress};
use thiserror::Error;
use tokio::sync::oneshot;
use tracing::{debug, warn};

use crate::state_chain_observer::client::{
	base_rpc_api, storage_api::StorageApi, SUBSTRATE_BEHAVIOUR,
};

use super::signer;

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

pub type ExtrinsicResult =
	Result<(H256, Vec<state_chain_runtime::RuntimeEvent>, DispatchInfo), ExtrinsicError>;

pub type RequestID = u64;

#[derive(Debug)]
pub struct Request {
	id: RequestID,
	pending_submissions: usize,
	pub allow_resubmits: bool,
	lifetime: std::ops::RangeToInclusive<cf_primitives::BlockNumber>,
	call: state_chain_runtime::RuntimeCall,
	result_sender: oneshot::Sender<ExtrinsicResult>,
}

#[derive(Debug)]
pub enum RequestStrategy {
	Submit(oneshot::Sender<H256>),
	Finalize,
}

pub struct Submission {
	lifetime: std::ops::RangeTo<cf_primitives::BlockNumber>,
	tx_hash: H256,
	request_id: RequestID,
}

pub struct SubmissionWatcher<BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static> {
	submissions_by_nonce: BTreeMap<state_chain_runtime::Index, Vec<Submission>>,
	pub anticipated_nonce: state_chain_runtime::Index,
	signer: signer::PairSigner<sp_core::sr25519::Pair>,
	finalized_nonce: state_chain_runtime::Index,
	finalized_block_hash: state_chain_runtime::Hash,
	finalized_block_number: state_chain_runtime::BlockNumber,
	runtime_version: sp_version::RuntimeVersion,
	genesis_hash: state_chain_runtime::Hash,
	extrinsic_lifetime: state_chain_runtime::BlockNumber,
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
		finalized_nonce: state_chain_runtime::Index,
		finalized_block_hash: state_chain_runtime::Hash,
		finalized_block_number: state_chain_runtime::BlockNumber,
		runtime_version: sp_version::RuntimeVersion,
		genesis_hash: state_chain_runtime::Hash,
		extrinsic_lifetime: state_chain_runtime::BlockNumber,
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
			Default::default(),
		)
	}

	pub fn finalized_nonce(&self) -> state_chain_runtime::Index {
		self.finalized_nonce
	}

	pub async fn submit_extrinsic_at_nonce(
		&mut self,
		request: &mut Request,
		nonce: state_chain_runtime::Index,
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
					self.submissions_by_nonce
						.entry(self.anticipated_nonce)
						.or_default()
						.push(Submission { lifetime, tx_hash, request_id: request.id });
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
			self.anticipated_nonce = state_chain_runtime::Index::max(self.anticipated_nonce, nonce);

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

					for submission in submissions {
						if let Some(request) = requests.get_mut(&submission.request_id) {
							request.pending_submissions -= 1;
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
									.map(|dispatch_info| (tx_hash, extrinsic_events, dispatch_info))
							});
						}
					}
				}
			}

			self.submissions_by_nonce.retain(|nonce, submissions| {
				assert!(self.finalized_nonce <= *nonce, "{SUBSTRATE_BEHAVIOUR}");

				submissions.retain(|submission| {
					let alive = submission.lifetime.contains(&(block.header.number + 1));

					if !alive {
						if let Some(request) = requests.get_mut(&submission.request_id) {
							request.pending_submissions -= 1;
						}
					}

					alive
				});

				!submissions.is_empty()
			});

			for (_request_id, request) in requests.drain_filter(|_request_id, request| {
				!request.lifetime.contains(&(block.header.number + 1)) ||
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

			// Has to be a separate loop from the above due to not being able to await inside
			// drain_filter
			for (_request_id, request) in requests.iter_mut() {
				if request.pending_submissions == 0 {
					debug!("Resubmitting extrinsic as all existing submissions have expired.");
					self.submit_extrinsic(request).await?;
				}
			}

			Ok(())
		}
	}
}
