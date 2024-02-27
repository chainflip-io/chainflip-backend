use std::{future::Future, sync::Arc, time::Duration};

use anyhow::{Context, Result};

use cf_primitives::EpochIndex;
use sol_rpc::{calls::GetGenesisHash, traits::CallApi as SolanaApi};
use utilities::task_scope::Scope;

use crate::{
	db::PersistentKeyDB,
	state_chain_observer::client::{
		chain_api::ChainApi,
		extrinsic_api::signed::SignedExtrinsicApi,
		storage_api::StorageApi,
		stream_api::{StreamApi, FINALIZED},
	},
	witness::{common::epoch_source::EpochSource, sol::epoch_stream::epoch_stream},
};

use super::common::epoch_source::EpochSourceBuilder;

mod deposit_addresses;
mod epoch_stream;
mod tracked_data;
mod zip_with_latest;

const SOLANA_SIGNATURES_FOR_TRANSACTION_PAGE_SIZE: usize = 100;
const SOLANA_SIGNATURES_FOR_TRANSACTION_POLL_INTERVAL: Duration = Duration::from_secs(5);
const SOLANA_CHAIN_TRACKER_SLEEP_INTERVAL: Duration = Duration::from_secs(5);
const SC_BLOCK_TIME: Duration = Duration::from_secs(6);

pub async fn start<SolanaClient, StateChainClient, StateChainStream, ProcessCall, ProcessingFut>(
	scope: &Scope<'_, anyhow::Error>,
	sol_client: SolanaClient,
	process_call: ProcessCall,
	state_chain_client: Arc<StateChainClient>,
	_state_chain_stream: StateChainStream,
	epoch_source: EpochSourceBuilder<'_, '_, StateChainClient, (), ()>,
	_db: Arc<PersistentKeyDB>,
) -> Result<()>
where
	SolanaClient: SolanaApi + Send + Sync + 'static,
	StateChainClient: StorageApi + ChainApi + SignedExtrinsicApi + 'static + Send + Sync,
	StateChainStream: StreamApi<FINALIZED> + Clone,
	ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
		+ Send
		+ Sync
		+ Clone
		+ 'static,
	ProcessingFut: Future<Output = ()> + Send + 'static,
{
	let sol_client = Arc::new(sol_client);

	let solana_genesis_hash = sol_client.call(GetGenesisHash::default()).await?;
	tracing::info!("Solana genesis hash: {}", solana_genesis_hash);

	let vault_address = state_chain_client
		.storage_value::<pallet_cf_environment::SolanaVaultAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.context("Failed to get Vault contract address from SC")?;

	tracing::info!("solana vault address: {}", vault_address);

	let epoch_source = EpochSource::from(epoch_source);

	scope.spawn(tracked_data::track_chain_state(
		epoch_stream(epoch_source.clone()).await,
		Arc::clone(&sol_client),
		process_call.clone(),
		state_chain_client.clone(),
	));

	scope.spawn(deposit_addresses::track_deposit_addresses(
		epoch_source,
		sol_client,
		process_call,
		state_chain_client,
	));

	Ok(())
}
