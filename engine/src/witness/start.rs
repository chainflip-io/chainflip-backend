use std::sync::Arc;

use utilities::task_scope::Scope;

use sol_rpc::traits::CallApi as SolanaApi;

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
	sol_client: impl SolanaApi + Send + Sync + 'static,
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
			// tracing::warn!("WITNESS: @{:?} — {:?}", epoch_index, call);
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
			// tracing::warn!("PREWITNESS: @{:?} — {:?}", epoch_index, call);
			let state_chain_client = state_chain_client.clone();
			async move {
				let _ = state_chain_client
					.finalize_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
						call: Box::new(
							pallet_cf_witnesser::Call::prewitness { call: Box::new(call) }.into(),
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
		prewitness_call.clone(),
		state_chain_client.clone(),
		state_chain_stream.clone(),
		unfinalised_state_chain_stream.clone(),
		epoch_source.clone(),
		db.clone(),
	);

	let start_dot = super::dot::start(
		scope,
		dot_client.clone(),
		witness_call.clone(),
		state_chain_client.clone(),
		state_chain_stream.clone(),
		epoch_source.clone(),
		db.clone(),
	);

	let start_sol = super::sol::start(
		scope,
		sol_client,
		witness_call,
		state_chain_client,
		state_chain_stream,
		epoch_source,
		db,
	);

	futures::future::try_join4(start_eth, start_btc, start_dot, start_sol).await?;

	Ok(())
}
