use std::{future::Future, sync::Arc, time::Duration};

use cf_chains::{sol::SolTrackedData, ChainState, Solana};
use cf_primitives::EpochIndex;
use futures::StreamExt;
use sol_rpc::calls::{GetExistingBlocks, GetSlot};
use state_chain_runtime::SolanaInstance;

use crate::{
	state_chain_observer::client::{
		chain_api::ChainApi, extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi,
	},
	witness::common::{epoch_source::EpochSource, ActiveAndFuture},
};

use super::{Result, SolanaApi, SOLANA_CHAIN_TRACKER_SLEEP_INTERVAL};

const SLOT_GRANULARITY: u64 = 25;

pub async fn collect_tracked_data<C: SolanaApi>(sol_client: C) -> Result<ChainState<Solana>> {
	let latest_slot = sol_client.call(GetSlot::default()).await?;
	let min_slot = latest_slot - (latest_slot % SLOT_GRANULARITY);
	let existing_slots = sol_client.call(GetExistingBlocks::range(min_slot, latest_slot)).await?;
	let reported_slot = existing_slots
		.first()
		.copied()
		.ok_or_else(|| anyhow::anyhow!("Come on! At least the `latest_slot` must exist!"))?;

	let chain_state = ChainState {
		block_height: reported_slot,
		tracked_data: SolTrackedData { ingress_fee: None, egress_fee: None },
	};

	Ok(chain_state)
}

pub async fn track_chain_state<SolanaClient, StateChainClient, ProcessCall, ProcessingFut>(
	epoch_source: EpochSource<(), ()>,
	sol_client: Arc<SolanaClient>,
	process_call: ProcessCall,
	_state_chain_client: Arc<StateChainClient>,
) -> Result<()>
where
	SolanaClient: SolanaApi + Send + Sync + 'static,
	StateChainClient: StorageApi + ChainApi + SignedExtrinsicApi + 'static + Send + Sync,
	ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
		+ Send
		+ Sync
		+ Clone
		+ 'static,
	ProcessingFut: Future<Output = ()> + Send + 'static,
{
	let ActiveAndFuture { active: active_epochs, future: upcoming_epochs } =
		epoch_source.into_stream().await;
	let mut upcoming_epochs = std::pin::pin!(upcoming_epochs);

	let mut current_epoch = active_epochs
		.last()
		.ok_or(anyhow::anyhow!("No active_epochs — empty iterator"))?;

	let mut ticks = tokio::time::interval(SOLANA_CHAIN_TRACKER_SLEEP_INTERVAL);
	let mut last_reported_height = None;

	loop {
		ticks.tick().await;

		if let Some(new_epoch) = tokio::time::timeout(Duration::ZERO, upcoming_epochs.next())
			.await
			.ok()
			.flatten()
		{
			current_epoch = new_epoch;
		}

		let chain_state = collect_tracked_data(&sol_client).await?;

		if last_reported_height.replace(chain_state.block_height) != Some(chain_state.block_height)
		{
			tracing::info!(
				"updating chain at {} state with {:?}",
				current_epoch.index,
				chain_state
			);

			let call = pallet_cf_chain_tracking::Call::<
				state_chain_runtime::Runtime,
				SolanaInstance,
			>::update_chain_state {
				new_chain_state: chain_state,
			};

			process_call(call.into(), current_epoch.index).await;
		}
	}
}
