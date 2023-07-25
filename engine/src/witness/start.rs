use std::sync::Arc;

use utilities::task_scope::Scope;

use crate::{
	db::PersistentKeyDB,
	settings::Settings,
	state_chain_observer::client::{
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi, StateChainStreamApi,
	},
};

use super::{epoch_source::EpochSource, vault::EthAssetApi};

use anyhow::Result;

/// Starts all the witnessing tasks.
pub async fn start<StateChainClient, StateChainStream>(
	scope: &Scope<'_, anyhow::Error>,
	settings: &Settings,
	state_chain_client: Arc<StateChainClient>,
	state_chain_stream: StateChainStream,
	db: Arc<PersistentKeyDB>,
) -> Result<()>
where
	StateChainStream: StateChainStreamApi + Clone,
	StateChainClient: StorageApi + EthAssetApi + SignedExtrinsicApi + 'static + Send + Sync,
{
	let initial_block_hash = state_chain_stream.cache().block_hash;
	let epoch_source =
		EpochSource::builder(scope, state_chain_stream.clone(), state_chain_client.clone())
			.await
			.participating(state_chain_client.account_id())
			.await;

	super::eth::start(
		scope,
		&settings.eth,
		state_chain_client.clone(),
		epoch_source.clone(),
		initial_block_hash,
		db.clone(),
	)
	.await?;

	super::btc::start(
		scope,
		&settings.btc,
		state_chain_client.clone(),
		state_chain_stream,
		epoch_source.clone(),
		db,
	)
	.await?;

	super::dot::start(scope, &settings.dot, state_chain_client, epoch_source).await?;

	Ok(())
}
