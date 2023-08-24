use std::sync::Arc;

use futures_core::Future;
use utilities::task_scope::Scope;

use crate::{
	btc::{retry_rpc::BtcRetryRpcClient, rpc::BtcRpcClient},
	db::PersistentKeyDB,
	dot::{http_rpc::DotHttpRpcClient, retry_rpc::DotRetryRpcClient, rpc::DotSubClient},
	eth::{
		retry_rpc::EthersRetryRpcClient,
		rpc::{EthRpcClient, ReconnectSubscriptionClient},
	},
	state_chain_observer::client::{
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi, StateChainStreamApi,
	},
};

use crate::state_chain_observer::client::chain_api::ChainApi;

use super::common::epoch_source::EpochSource;

use anyhow::Result;

/// Starts all the witnessing tasks.
pub async fn start<StateChainClient, StateChainStream>(
	scope: &Scope<'_, anyhow::Error>,
	eth_client: EthersRetryRpcClient<
		impl Future<Output = EthRpcClient> + Send,
		impl Future<Output = ReconnectSubscriptionClient> + Send,
	>,
	btc_client: BtcRetryRpcClient<impl Future<Output = BtcRpcClient> + Send>,
	dot_client: DotRetryRpcClient<
		impl Future<Output = DotHttpRpcClient> + Send,
		impl Future<Output = DotSubClient> + Send,
	>,
	state_chain_client: Arc<StateChainClient>,
	state_chain_stream: StateChainStream,
	db: Arc<PersistentKeyDB>,
) -> Result<()>
where
	StateChainStream: StateChainStreamApi + Clone,
	StateChainClient: StorageApi + ChainApi + SignedExtrinsicApi + 'static + Send + Sync,
{
	let epoch_source =
		EpochSource::builder(scope, state_chain_stream.clone(), state_chain_client.clone())
			.await
			.participating(state_chain_client.account_id())
			.await;

	super::eth::start(
		scope,
		eth_client,
		state_chain_client.clone(),
		state_chain_stream.clone(),
		epoch_source.clone(),
		db.clone(),
	)
	.await?;

	super::btc::start(
		scope,
		btc_client,
		state_chain_client.clone(),
		state_chain_stream.clone(),
		epoch_source.clone(),
		db.clone(),
	)
	.await?;

	super::dot::start(scope, dot_client, state_chain_client, state_chain_stream, epoch_source, db)
		.await?;

	Ok(())
}
