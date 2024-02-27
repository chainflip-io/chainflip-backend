use std::sync::Arc;

use utilities::task_scope::Scope;

use crate::{
	btc::retry_rpc::BtcRetryRpcClient,
	db::PersistentKeyDB,
	dot::retry_rpc::DotRetryRpcClient,
	eth::{retry_rpc::EthRetryRpcClient, rpc::EthRpcSigningClient},
	state_chain_observer::client::{
		extrinsic_api::signed::SignedExtrinsicApi,
		storage_api::StorageApi,
		stream_api::{StreamApi, FINALIZED, UNFINALIZED},
	},
};

use crate::state_chain_observer::client::chain_api::ChainApi;

use super::common::epoch_source::EpochSource;

use anyhow::Result;

/// Starts all the witnessing tasks.
// It's important that this function is not blocking, at any point, even if there is no connection
// to any or all chains. This implies that the `start` function for each chain should not be
// blocking. The chains must be able to witness independently, and if this blocks at any
// point it means that on start up this will block, and the state chain observer will not start.
pub async fn start<StateChainClient>(
	scope: &Scope<'_, anyhow::Error>,
	eth_client: EthRetryRpcClient<EthRpcSigningClient>,
	btc_client: BtcRetryRpcClient,
	dot_client: DotRetryRpcClient,
	state_chain_client: Arc<StateChainClient>,
	state_chain_stream: impl StreamApi<FINALIZED> + Clone,
	unfinalised_state_chain_stream: impl StreamApi<UNFINALIZED> + Clone,
	db: Arc<PersistentKeyDB>,
) -> Result<()>
where
	StateChainClient: StorageApi + ChainApi + SignedExtrinsicApi + 'static + Send + Sync,
{
	let epoch_source =
		EpochSource::builder(scope, state_chain_stream.clone(), state_chain_client.clone())
			.await
			.participating(state_chain_client.account_id())
			.await;

	let witness_call = {
		let state_chain_client = state_chain_client.clone();
		move |call, epoch_index| {
			let state_chain_client = state_chain_client.clone();
			async move {
				let _ = state_chain_client
					.finalize_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
						call: Box::new(call),
						epoch_index,
					})
					.await;
			}
		}
	};

	let prewitness_call = {
		let state_chain_client = state_chain_client.clone();
		move |call, epoch_index| {
			let state_chain_client = state_chain_client.clone();
			async move {
				let _ = state_chain_client
					.finalize_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
						call: Box::new(
							pallet_cf_witnesser::Call::prewitness_and_execute {
								call: Box::new(call),
							}
							.into(),
						),
						epoch_index,
					})
					.await;
			}
		}
	};

	let start_eth = super::eth::start(
		scope,
		eth_client,
		witness_call.clone(),
		state_chain_client.clone(),
		state_chain_stream.clone(),
		epoch_source.clone(),
		db.clone(),
	);

	let start_btc = super::btc::start(
		scope,
		btc_client,
		witness_call.clone(),
		prewitness_call,
		state_chain_client.clone(),
		state_chain_stream.clone(),
		unfinalised_state_chain_stream.clone(),
		epoch_source.clone(),
		db.clone(),
	);

	let start_dot = super::dot::start(
		scope,
		dot_client,
		witness_call,
		state_chain_client,
		state_chain_stream,
		epoch_source,
		db,
	);

	futures::future::try_join3(start_eth, start_btc, start_dot).await?;

	Ok(())
}
