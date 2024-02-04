use std::{future::Future, sync::Arc};

use anyhow::Result;

use cf_primitives::EpochIndex;
use utilities::task_scope::Scope;

use crate::{
	db::PersistentKeyDB,
	state_chain_observer::client::{
		chain_api::ChainApi,
		extrinsic_api::signed::SignedExtrinsicApi,
		storage_api::StorageApi,
		stream_api::{StreamApi, FINALIZED},
	},
};

use super::common::epoch_source::EpochSourceBuilder;

mod sol_source;

pub async fn start<StateChainClient, StateChainStream, ProcessCall, ProcessingFut>(
	_scope: &Scope<'_, anyhow::Error>,
	// sol_client: ???,
	_process_call: ProcessCall,
	_state_chain_client: Arc<StateChainClient>,
	_state_chain_stream: StateChainStream,
	_epoch_source: EpochSourceBuilder<'_, '_, StateChainClient, (), ()>,
	_db: Arc<PersistentKeyDB>,
) -> Result<()>
where
	StateChainClient: StorageApi + ChainApi + SignedExtrinsicApi + 'static + Send + Sync,
	StateChainStream: StreamApi<FINALIZED> + Clone,
	ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
		+ Send
		+ Sync
		+ Clone
		+ 'static,
	ProcessingFut: Future<Output = ()> + Send + 'static,
{
	Ok(())
}
