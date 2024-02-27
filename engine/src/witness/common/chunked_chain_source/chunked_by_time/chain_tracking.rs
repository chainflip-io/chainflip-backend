use std::sync::Arc;

use cf_chains::ChainState;
use state_chain_runtime::PalletInstanceAlias;

use crate::{
	state_chain_observer::client::extrinsic_api::signed::SignedExtrinsicApi,
	witness::common::{chain_source::Header, RuntimeCallHasChain, RuntimeHasChain},
};
use cf_chains::Chain;
use utilities::metrics::CHAIN_TRACKING;

use super::{builder::ChunkedByTimeBuilder, ChunkedByTime};

#[async_trait::async_trait]
pub trait GetTrackedData<C: cf_chains::Chain, Hash, Data>: Send + Sync + Clone {
	async fn get_tracked_data(
		&self,
		header: &Header<C::ChainBlockNumber, Hash, Data>,
	) -> Result<C::TrackedData, anyhow::Error>;
}

impl<Inner: ChunkedByTime> ChunkedByTimeBuilder<Inner> {
	pub fn chain_tracking<StateChainClient, TrackedDataClient>(
		self,
		state_chain_client: Arc<StateChainClient>,
		tracked_data_client: TrackedDataClient,
	) -> ChunkedByTimeBuilder<impl ChunkedByTime>
	where
		Inner: ChunkedByTime,
		StateChainClient: SignedExtrinsicApi + Send + Sync + 'static,
		TrackedDataClient: GetTrackedData<Inner::Chain, Inner::Hash, Inner::Data>,
		state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
		state_chain_runtime::RuntimeCall:
			RuntimeCallHasChain<state_chain_runtime::Runtime, Inner::Chain>,
	{
		self.latest_then(move |epoch, header| {
			let state_chain_client = state_chain_client.clone();
			let tracked_data_client = tracked_data_client.clone();
			async move {
				let tracked_data = tracked_data_client.get_tracked_data(&header).await?;
				tracing::info!("tracked-data: {:?}", tracked_data);
				let call: Box<state_chain_runtime::RuntimeCall> = Box::new(
					pallet_cf_chain_tracking::Call::<
						state_chain_runtime::Runtime,
						<Inner::Chain as PalletInstanceAlias>::Instance,
					>::update_chain_state {
						new_chain_state: ChainState { block_height: header.index, tracked_data },
					}
					.into(),
				);
				state_chain_client
					.finalize_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
						call,
						epoch_index: epoch.index,
					})
					.await;
				CHAIN_TRACKING.set(&[Inner::Chain::NAME], Into::<u64>::into(header.index));
				Ok::<_, anyhow::Error>(header.data)
			}
		})
	}
}
