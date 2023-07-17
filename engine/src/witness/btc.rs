use std::sync::Arc;

use utilities::task_scope::Scope;

use crate::{
	btc::{retry_rpc::BtcRetryRpcClient, rpc::BtcRpcClient},
	settings::{self},
	state_chain_observer::client::{
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi,
	},
};

use super::{
	chain_source::{btc_source::BtcSource, extension::ChainSourceExt},
	epoch_source::EpochSource,
};

use anyhow::Result;

pub async fn start<StateChainClient>(
	scope: &Scope<'_, anyhow::Error>,
	settings: &settings::Btc,
	state_chain_client: Arc<StateChainClient>,
	epoch_source: EpochSource<'_, '_, StateChainClient, (), ()>,
) -> Result<()>
where
	StateChainClient: StorageApi + SignedExtrinsicApi + 'static + Send + Sync,
{
	let btc_client = BtcRetryRpcClient::new(scope, BtcRpcClient::new(settings)?);

	let btc_witnessing = BtcSource::new(btc_client.clone())
		.shared(scope)
		.chunk_by_time(epoch_source)
		.await
		.chain_tracking(state_chain_client, btc_client)
		.run();

	scope.spawn(async move {
		btc_witnessing.await;
		Ok(())
	});

	Ok(())
}
