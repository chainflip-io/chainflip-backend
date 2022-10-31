use std::sync::atomic::Ordering;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use codec::Encode;
use frame_support::pallet_prelude::InvalidTransaction;
use frame_system::Phase;
use futures::{Stream, StreamExt};
use jsonrpsee::{
	core::Error as RpcError,
	types::{error::CallError, ErrorObject, ErrorObjectOwned},
};
use sp_core::H256;
use sp_runtime::{
	generic::Era,
	traits::{BlakeTwo256, Hash},
	MultiAddress,
};
use sp_version::RuntimeVersion;
use state_chain_runtime::AccountId;

use crate::constants::MAX_EXTRINSIC_RETRY_ATTEMPTS;

use super::{base_rpc_api::BaseRpcApi, storage_api::SafeStorageApi};

// Note 'static on the generics in this trait are only required for mockall to mock it
#[async_trait]
pub trait ExtrinsicApi {
	fn account_id(&self) -> AccountId;

	async fn submit_signed_extrinsic<Call>(
		&self,
		call: Call,
		logger: &slog::Logger,
	) -> Result<H256>
	where
		Call: Into<state_chain_runtime::Call> + Clone + std::fmt::Debug + Send + Sync + 'static;

	async fn submit_unsigned_extrinsic<Call>(
		&self,
		call: Call,
		logger: &slog::Logger,
	) -> Result<H256>
	where
		Call: Into<state_chain_runtime::Call> + Clone + std::fmt::Debug + Send + Sync + 'static;

	async fn watch_submitted_extrinsic<BlockStream>(
		&self,
		extrinsic_hash: state_chain_runtime::Hash,
		block_stream: &mut BlockStream,
	) -> Result<Vec<state_chain_runtime::Event>>
	where
		BlockStream:
			Stream<Item = anyhow::Result<state_chain_runtime::Header>> + Unpin + Send + 'static;
}

fn invalid_err_obj(invalid_reason: InvalidTransaction) -> ErrorObjectOwned {
	ErrorObject::owned(1010, "Invalid Transaction", Some(<&'static str>::from(invalid_reason)))
}

impl super::StateChainClient {
	fn create_and_sign_extrinsic(
		&self,
		call: state_chain_runtime::Call,
		runtime_version: &RuntimeVersion,
		genesis_hash: state_chain_runtime::Hash,
		nonce: state_chain_runtime::Index,
	) -> state_chain_runtime::UncheckedExtrinsic {
		let extra: state_chain_runtime::SignedExtra = (
			frame_system::CheckNonZeroSender::new(),
			frame_system::CheckSpecVersion::new(),
			frame_system::CheckTxVersion::new(),
			frame_system::CheckGenesis::new(),
			frame_system::CheckEra::from(Era::Immortal),
			frame_system::CheckNonce::from(nonce),
			frame_system::CheckWeight::new(),
			// This is the tx fee tip. Normally this determines transaction priority. We currently
			// ignore this in the runtime but it needs to be set to some default value.
			state_chain_runtime::ChargeTransactionPayment::from(0),
		);
		let additional_signed = (
			(),
			runtime_version.spec_version,
			runtime_version.transaction_version,
			genesis_hash,
			genesis_hash,
			(),
			(),
			(),
		);

		let signed_payload = state_chain_runtime::SignedPayload::from_raw(
			call.clone(),
			extra.clone(),
			additional_signed,
		);
		let signature = signed_payload.using_encoded(|bytes| self.signer.sign(bytes));

		state_chain_runtime::UncheckedExtrinsic::new_signed(
			call,
			MultiAddress::Id(self.signer.account_id.clone()),
			signature,
			extra,
		)
	}
}

#[async_trait]
impl ExtrinsicApi for super::StateChainClient {
	fn account_id(&self) -> AccountId {
		self.signer.account_id.clone()
	}

	/// Sign and submit an extrinsic, retrying up to [MAX_EXTRINSIC_RETRY_ATTEMPTS] times if it
	/// fails on an invalid nonce.
	async fn submit_signed_extrinsic<Call>(&self, call: Call, logger: &slog::Logger) -> Result<H256>
	where
		Call: Into<state_chain_runtime::Call> + Clone + std::fmt::Debug + Send + Sync + 'static,
	{
		for _ in 0..MAX_EXTRINSIC_RETRY_ATTEMPTS {
			// use the previous value but increment it for the next thread that loads/fetches it
			let nonce = self.nonce.fetch_add(1, Ordering::Relaxed);
			let runtime_version = { self.runtime_version.read().await.clone() };
			match self
				.base_rpc_client
				.submit_extrinsic(self.create_and_sign_extrinsic(
					call.clone().into(),
					&runtime_version,
					self.genesis_hash,
					nonce,
				))
				.await
			{
				Ok(tx_hash) => {
					slog::info!(
						logger,
						"{:?} submitted successfully with tx_hash: {:#x}",
						&call,
						tx_hash
					);
					return Ok(tx_hash)
				},
				Err(rpc_err) => match rpc_err {
					// This occurs when a transaction with the same nonce is in the transaction pool
					// (and the priority is <= priority of that existing tx)
					RpcError::Call(CallError::Custom(ref obj)) if obj.code() == 1014 => {
						slog::error!(
							logger,
							"Extrinsic submission failed with nonce: {}. Error: {:?}",
							nonce,
							rpc_err
						);
					},
					// This occurs when the nonce has already been *consumed* i.e a transaction with
					// that nonce is in a block
					RpcError::Call(CallError::Custom(ref obj))
						if obj == &invalid_err_obj(InvalidTransaction::Stale) =>
					{
						slog::error!(
							logger,
							"Extrinsic submission failed with nonce: {}. Error: {:?}",
							nonce,
							rpc_err
						);
					},
					RpcError::Call(CallError::Custom(ref obj))
						if obj == &invalid_err_obj(InvalidTransaction::BadProof) =>
					{
						slog::error!(
                            logger,
                            "Extrinsic submission failed with nonce: {}. Error: {:?}. Refetching the runtime version.",
                            nonce,
                            rpc_err
                        );

						// we want to reset the nonce, either for the next extrinsic, or for when
						// we retry this one, with the updated runtime_version
						self.nonce.fetch_sub(1, Ordering::Relaxed);

						let latest_block_hash =
							self.base_rpc_client.latest_finalized_block_hash().await?;

						let runtime_version =
							self.base_rpc_client.fetch_runtime_version(latest_block_hash).await?;

						{
							let runtime_version_locked =
								{ self.runtime_version.read().await.clone() };

							if runtime_version_locked == runtime_version {
								slog::warn!(logger, "Fetched RuntimeVersion of {:?} is the same as the previous RuntimeVersion. This is not expected.", &runtime_version);
								// break, as the error is now very unlikely to be solved by fetching
								// again
								break
							}

							*(self.runtime_version.write().await) = runtime_version;
						}
						// don't `return`, therefore go back to the top of the loop and retry
						// sending the transaction
					},
					err => {
						slog::error!(
							logger,
							"Extrinsic failed with error: {}. Extrinsic: {:?}",
							err,
							&call,
						);
						self.nonce.fetch_sub(1, Ordering::Relaxed);
						return Err(err.into())
					},
				},
			}
		}
		slog::error!(logger, "Exceeded maximum number of retry attempts");
		Err(anyhow!("Exceeded maximum number of retry attempts",))
	}

	/// Submit an unsigned extrinsic.
	async fn submit_unsigned_extrinsic<Call>(
		&self,
		call: Call,
		logger: &slog::Logger,
	) -> Result<H256>
	where
		Call: Into<state_chain_runtime::Call> + std::fmt::Debug + Clone + Send + Sync + 'static,
	{
		let extrinsic = state_chain_runtime::UncheckedExtrinsic::new_unsigned(call.clone().into());
		let expected_hash = BlakeTwo256::hash_of(&extrinsic);
		match self.base_rpc_client.submit_extrinsic(extrinsic).await {
			Ok(tx_hash) => {
				slog::info!(
					logger,
					"Unsigned extrinsic {:?} submitted successfully with tx_hash: {:#x}",
					&call,
					tx_hash
				);
				assert_eq!(
					tx_hash, expected_hash,
					"tx_hash returned from RPC does not match expected hash"
				);
				Ok(tx_hash)
			},
			Err(rpc_err) => {
				match rpc_err {
					// POOL_ALREADY_IMPORTED error occurs when the transaction is already in the
					// pool More than one node can submit the same unsigned extrinsic. E.g. in the
					// case of a threshold signature success. Thus, if we get a "Transaction already
					// in pool" "error" we know that this particular extrinsic has already been
					// submitted. And so we can ignore the error and return the transaction hash
					RpcError::Call(CallError::Custom(ref obj)) if obj.code() == 1013 => {
						slog::trace!(
							logger,
							"Unsigned extrinsic {:?} with tx_hash {:#x} already in pool.",
							&call,
							expected_hash
						);
						Ok(expected_hash)
					},
					_ => {
						slog::error!(
							logger,
							"Unsigned extrinsic failed with error: {}. Extrinsic: {:?}",
							rpc_err,
							&call
						);
						Err(rpc_err.into())
					},
				}
			},
		}
	}

	/// Watches *only* submitted extrinsics. I.e. Cannot watch for chain called extrinsics.
	async fn watch_submitted_extrinsic<BlockStream>(
		&self,
		extrinsic_hash: state_chain_runtime::Hash,
		block_stream: &mut BlockStream,
	) -> Result<Vec<state_chain_runtime::Event>>
	where
		BlockStream:
			Stream<Item = anyhow::Result<state_chain_runtime::Header>> + Unpin + Send + 'static,
	{
		while let Some(result_header) = block_stream.next().await {
			let header = result_header?;
			let block_hash = header.hash();
			if let Some(signed_block) = self.base_rpc_client.block(block_hash).await? {
				match signed_block.block.extrinsics.iter().position(|ext| {
					let hash = BlakeTwo256::hash_of(ext);
					hash == extrinsic_hash
				}) {
					Some(extrinsic_index_found) => {
						let events_for_block = self
							.get_storage_value::<frame_system::Events<state_chain_runtime::Runtime>>(
								block_hash,
							)
							.await?;
						return Ok(events_for_block
							.into_iter()
							.filter_map(|event_record| {
								if let Phase::ApplyExtrinsic(i) = event_record.phase {
									if i as usize != extrinsic_index_found {
										None
									} else {
										Some(event_record.event)
									}
								} else {
									None
								}
							})
							.collect::<Vec<_>>())
					},
					None => continue,
				}
			};
		}
		Err(anyhow!("Block stream loop exited, no event found",))
	}
}
