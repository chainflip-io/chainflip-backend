use std::{collections::BTreeMap, sync::Arc};

use anyhow::{anyhow, Result};
use frame_support::pallet_prelude::InvalidTransaction;
use itertools::Itertools;
use sp_core::H256;
use sp_runtime::{traits::Hash, MultiAddress};
use tracing::{debug, warn};

use crate::state_chain_observer::client::{
	base_rpc_api, storage_api::StorageApi, SUBSTRATE_BEHAVIOUR,
};

use super::signer;

pub struct Submission<Identity> {
	lifetime: std::ops::RangeTo<cf_primitives::BlockNumber>,
	tx_hash: H256,
	pub identity: Identity,
}

pub struct SubmissionWatcher<
	Identity,
	BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static,
> {
	submissions_by_nonce: BTreeMap<state_chain_runtime::Index, Vec<Submission<Identity>>>,
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

impl<Identity: Copy, BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static>
	SubmissionWatcher<Identity, BaseRpcClient>
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
	) -> Self {
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
		}
	}

	pub fn finalized_nonce(&self) -> state_chain_runtime::Index {
		self.finalized_nonce
	}

	pub async fn submit_extrinsic_at_nonce(
		&mut self,
		call: state_chain_runtime::RuntimeCall,
		nonce: state_chain_runtime::Index,
		identity: Identity,
	) -> Result<Result<H256, SubmissionLogicError>, anyhow::Error> {
		loop {
			let (signed_extrinsic, lifetime) = self.signer.new_signed_extrinsic(
				call.clone(),
				&self.runtime_version,
				self.genesis_hash,
				self.finalized_block_hash,
				self.finalized_block_number,
				self.extrinsic_lifetime,
				nonce,
			);

			match self.base_rpc_client.submit_extrinsic(signed_extrinsic).await {
				Ok(tx_hash) => {
					self.submissions_by_nonce
						.entry(self.anticipated_nonce)
						.or_default()
						.push(Submission { lifetime, tx_hash, identity });
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

	pub async fn submit_extrinsic(
		&mut self,
		call: state_chain_runtime::RuntimeCall,
		identity: Identity,
	) -> Result<H256, anyhow::Error> {
		Ok(loop {
			match self
				.submit_extrinsic_at_nonce(call.clone(), self.anticipated_nonce, identity)
				.await?
			{
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

	pub async fn on_block_finalized<State, OnSubmissionFinalized, OnSubmissionDeath>(
		&mut self,
		block_hash: H256,
		state: &mut State,
		mut on_submission_finalized: OnSubmissionFinalized,
		mut on_submission_death: OnSubmissionDeath,
	) -> Result<()>
	where
		OnSubmissionFinalized: FnMut(
			&mut State,
			H256,
			&state_chain_runtime::RuntimeCall,
			Vec<state_chain_runtime::RuntimeEvent>,
			Vec<Submission<Identity>>,
		),
		OnSubmissionDeath: FnMut(&mut State, &Submission<Identity>),
	{
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
				.iter()
				.filter_map(|event_record| match &**event_record {
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
			{
				let extrinsic = &block.extrinsics[*extrinsic_index as usize];
				// TODO: Assumption needs checking
				if let Some(mut submissions) = extrinsic.signature.as_ref().and_then(
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

					for submission in submissions.drain_filter(|submission| {
						// Note: It is technically possible for a hash collision to
						// occur, but it is so unlikely it is effectively
						// impossible. If it where to occur this code would not
						// notice the extrinsic was not actually the requested one,
						// but otherwise would continue to work.
						submission.tx_hash != tx_hash
					}) {
						on_submission_death(state, &submission);
					}

					if !submissions.is_empty() {
						assert!(submissions
							.iter()
							.all(|submission| submission.lifetime.contains(&block.header.number)));

						on_submission_finalized(
							state,
							tx_hash,
							&extrinsic.function,
							extrinsic_events
								.map(|(_extrinsics_index, event)| event.clone())
								.collect::<Vec<_>>(),
							submissions, /* Note it is possible for this to
							              * contain more than one element, for
							              * example by submitting the same
							              * extrinsic repeatedly */
						);
					}
				}
			}

			self.submissions_by_nonce.retain(|nonce, submissions| {
				assert!(self.finalized_nonce <= *nonce, "{SUBSTRATE_BEHAVIOUR}");

				submissions.retain(|submission| {
					let alive = submission.lifetime.contains(&(block.header.number + 1));

					if !alive {
						on_submission_death(state, submission);
					}

					alive
				});

				!submissions.is_empty()
			});

			Ok(())
		}
	}
}
