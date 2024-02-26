use std::{future::Future, sync::Arc};

use futures::{FutureExt, Stream, StreamExt, TryStreamExt};

use cf_chains::{sol::SolTrackedData, ChainState, Solana};
use cf_primitives::EpochIndex;
use sol_rpc::calls::{GetExistingBlocks, GetSlot};
use sol_watch::deduplicate_stream::DeduplicateStreamExt;
use state_chain_runtime::SolanaInstance;
use tokio_stream::wrappers::IntervalStream;

use crate::{
	state_chain_observer::client::{
		chain_api::ChainApi, extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi,
	},
	witness::common::epoch_source::Epoch,
};

use super::{
	zip_with_latest::TryZipLatestExt, Result, SolanaApi, SOLANA_CHAIN_TRACKER_SLEEP_INTERVAL,
};

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

pub async fn track_chain_state<
	EpochStream,
	SolanaClient,
	StateChainClient,
	ProcessCall,
	ProcessingFut,
>(
	epoch_stream: EpochStream,
	sol_client: Arc<SolanaClient>,
	process_call: ProcessCall,
	_state_chain_client: Arc<StateChainClient>,
) -> Result<()>
where
	EpochStream: Stream<Item = Epoch<(), ()>>,
	SolanaClient: SolanaApi + Send + Sync + 'static,
	StateChainClient: StorageApi + ChainApi + SignedExtrinsicApi + 'static + Send + Sync,
	ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
		+ Send
		+ Sync
		+ Clone
		+ 'static,
	ProcessingFut: Future<Output = ()> + Send + 'static,
{
	IntervalStream::new(tokio::time::interval(SOLANA_CHAIN_TRACKER_SLEEP_INTERVAL))
		.then(|_| collect_tracked_data(&sol_client))
		.deduplicate(1, |r| r.as_ref().ok().map(|cs| cs.block_height), |_, _| ())
		.try_zip_latest(epoch_stream)
		.try_for_each(|(new_chain_state, epoch)| {
			tracing::info!("updating chain-state at #{} with {:?}", epoch.index, new_chain_state);
			let update_chain_state = pallet_cf_chain_tracking::Call::<
				state_chain_runtime::Runtime,
				SolanaInstance,
			>::update_chain_state {
				new_chain_state,
			};
			process_call(update_chain_state.into(), epoch.index).map(Ok)
		})
		.await?;

	Ok(())
}
