use std::sync::Arc;

use pallet_cf_chain_tracking::ChainState;
use state_chain_runtime::PalletInstanceAlias;

use crate::witness::chunked_chain_source::Builder;

use crate::{
	state_chain_observer::client::extrinsic_api::signed::SignedExtrinsicApi,
	witness::common::{RuntimeCallHasChain, RuntimeHasChain},
};

use super::{ChunkedByTime, ChunkedByTimeAlias, Generic};

#[async_trait::async_trait]
pub trait GetTrackedData<C: cf_chains::Chain>: Send + Sync + Clone {
	async fn get_tracked_data(&self, block_number: C::ChainBlockNumber) -> C::TrackedData;
}

impl<Inner: ChunkedByTime> Builder<Generic<Inner>> {
	pub fn chain_tracking<StateChainClient, TrackedDataClient>(
		self,
		state_chain_client: Arc<StateChainClient>,
		tracked_data_client: TrackedDataClient,
	) -> Builder<impl ChunkedByTimeAlias>
	where
		Inner: ChunkedByTime,
		StateChainClient: SignedExtrinsicApi + Send + Sync + 'static,
		TrackedDataClient: GetTrackedData<Inner::Chain>,
		state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
		state_chain_runtime::RuntimeCall:
			RuntimeCallHasChain<state_chain_runtime::Runtime, Inner::Chain>,
	{
		self.then(move |epoch, header| {
			let state_chain_client = state_chain_client.clone();
			let tracked_data_client = tracked_data_client.clone();
			async move {
				let call: Box<state_chain_runtime::RuntimeCall> = Box::new(
					pallet_cf_chain_tracking::Call::<
						state_chain_runtime::Runtime,
						<Inner::Chain as PalletInstanceAlias>::Instance,
					>::update_chain_state {
						new_chain_state: ChainState {
							block_height: header.index,
							tracked_data: tracked_data_client.get_tracked_data(header.index).await,
						},
					}
					.into(),
				);
				state_chain_client
					.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
						call,
						epoch_index: epoch.index,
					})
					.await;

				header.data
			}
		})
	}
}
