use anyhow::{anyhow, Result};
use async_trait::async_trait;
use frame_system::Phase;
use futures::{Stream, StreamExt};
use sp_core::H256;
use sp_runtime::traits::{BlakeTwo256, Hash};
use state_chain_runtime::AccountId;
use tokio::sync::{mpsc, oneshot};

use super::storage_api::StorageApi;

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
		Call: Into<state_chain_runtime::RuntimeCall>
			+ Clone
			+ std::fmt::Debug
			+ Send
			+ Sync
			+ 'static;

	async fn submit_unsigned_extrinsic<Call>(
		&self,
		call: Call,
		logger: &slog::Logger,
	) -> Result<H256>
	where
		Call: Into<state_chain_runtime::RuntimeCall>
			+ Clone
			+ std::fmt::Debug
			+ Send
			+ Sync
			+ 'static;

	async fn watch_submitted_extrinsic<BlockStream>(
		&self,
		extrinsic_hash: state_chain_runtime::Hash,
		block_stream: &mut BlockStream,
	) -> Result<Vec<state_chain_runtime::RuntimeEvent>>
	where
		BlockStream: Stream<Item = state_chain_runtime::Header> + Unpin + Send + 'static;
}

impl<BaseRpcApi: super::base_rpc_api::BaseRpcApi + Send + Sync + 'static>
	super::StateChainClient<BaseRpcApi>
{
	async fn submit_extrinsic<Call>(
		request_sender: &mpsc::UnboundedSender<(
			state_chain_runtime::RuntimeCall,
			oneshot::Sender<Result<H256, anyhow::Error>>,
		)>,
		call: Call,
		logger: &slog::Logger,
	) -> Result<H256, anyhow::Error>
	where
		Call: Into<state_chain_runtime::RuntimeCall> + Clone + std::fmt::Debug,
	{
		let (extrinsic_result_sender, extrinsic_result_receiver) = oneshot::channel();

		{
			let _result = request_sender.send((call.clone().into(), extrinsic_result_sender));
		}

		let extrinsic_result = extrinsic_result_receiver.await.expect("Backend failed"); // TODO: This type of error in the codebase is currently handled inconsistently

		match &extrinsic_result {
			Ok(tx_hash) => {
				slog::info!(
					logger,
					"{:?} submission succeeded with tx_hash: {:#x}",
					&call,
					tx_hash
				);
			},
			Err(error) => {
				slog::error!(logger, "{:?} submission failed with error: {}", &call, error);
			},
		}

		extrinsic_result
	}
}

#[async_trait]
impl<BaseRpcApi: super::base_rpc_api::BaseRpcApi + Send + Sync + 'static> ExtrinsicApi
	for super::StateChainClient<BaseRpcApi>
{
	fn account_id(&self) -> AccountId {
		self.account_id.clone()
	}

	/// Sign and submit an extrinsic, retrying up to [MAX_EXTRINSIC_RETRY_ATTEMPTS] times if it
	/// fails on an invalid nonce.
	async fn submit_signed_extrinsic<Call>(&self, call: Call, logger: &slog::Logger) -> Result<H256>
	where
		Call: Into<state_chain_runtime::RuntimeCall>
			+ Clone
			+ std::fmt::Debug
			+ Send
			+ Sync
			+ 'static,
	{
		Self::submit_extrinsic(&self.signed_extrinsic_request_sender, call, logger).await
	}

	/// Submit an unsigned extrinsic.
	async fn submit_unsigned_extrinsic<Call>(
		&self,
		call: Call,
		logger: &slog::Logger,
	) -> Result<H256>
	where
		Call: Into<state_chain_runtime::RuntimeCall>
			+ std::fmt::Debug
			+ Clone
			+ Send
			+ Sync
			+ 'static,
	{
		Self::submit_extrinsic(&self.unsigned_extrinsic_request_sender, call, logger).await
	}

	/// Watches *only* submitted extrinsics. I.e. Cannot watch for chain called extrinsics.
	async fn watch_submitted_extrinsic<BlockStream>(
		&self,
		extrinsic_hash: state_chain_runtime::Hash,
		block_stream: &mut BlockStream,
	) -> Result<Vec<state_chain_runtime::RuntimeEvent>>
	where
		BlockStream: Stream<Item = state_chain_runtime::Header> + Unpin + Send + 'static,
	{
		while let Some(header) = block_stream.next().await {
			let block_hash = header.hash();
			if let Some(signed_block) = self.base_rpc_client.block(block_hash).await? {
				match signed_block.block.extrinsics.iter().position(|ext| {
					let hash = BlakeTwo256::hash_of(ext);
					hash == extrinsic_hash
				}) {
					Some(extrinsic_index_found) => {
						let events_for_block = self
							.storage_value::<frame_system::Events<state_chain_runtime::Runtime>>(
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
