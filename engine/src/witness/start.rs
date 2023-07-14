use std::sync::Arc;

use utilities::task_scope::Scope;

use crate::{
	settings::Settings,
	state_chain_observer::client::{
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi, StateChainStreamApi,
	},
};

/// Starts all the witnessing tasks.
pub async fn start<StateChainClient, StateChainStream>(
	scope: &Scope<'_, anyhow::Error>,
	settings: &Settings,
	state_chain_client: Arc<StateChainClient>,
	state_chain_stream: StateChainStream,
) where
	StateChainStream: StateChainStreamApi,
	StateChainClient: StorageApi + SignedExtrinsicApi + 'static + Send + Sync,
{
	super::eth::start(scope, &settings.eth, state_chain_client, state_chain_stream).await;
}
