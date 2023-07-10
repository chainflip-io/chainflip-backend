use std::sync::Arc;

use pallet_cf_chain_tracking::ChainState;
use state_chain_runtime::PalletInstanceAlias;

use crate::witness::{chain_source::Header, chunked_chain_source::map::Map, epoch_source::Epoch};

use crate::{
	state_chain_observer::client::extrinsic_api::signed::SignedExtrinsicApi,
	witness::common::{RuntimeCallHasChain, RuntimeHasChain},
};

use super::ChunkedByTime;

#[async_trait::async_trait]
pub trait GetTrackedData<C: cf_chains::Chain>: Send + Sync + Clone {
	async fn get_tracked_data(&self, block_number: C::ChainBlockNumber) -> C::TrackedData;
}

pub async fn chain_tracking<Inner, StateChainClient, TrackedDataClient>(
	inner: Inner,
	state_chain_client: Arc<StateChainClient>,
	tracked_data_client: TrackedDataClient,
) -> impl ChunkedByTime<
	Index = Inner::Index,
	Hash = Inner::Hash,
	Data = Inner::Data,
	Chain = Inner::Chain,
	Parameters = Inner::Parameters,
>
where
	Inner: ChunkedByTime,
	StateChainClient: SignedExtrinsicApi + Send + Sync + 'static,
	TrackedDataClient: GetTrackedData<Inner::Chain>,
	state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
	state_chain_runtime::RuntimeCall:
		RuntimeCallHasChain<state_chain_runtime::Runtime, Inner::Chain>,
{
	Map::new(super::Generic(inner), move |epoch: Epoch<(), ()>, header: Header<_, _, _>| {
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
