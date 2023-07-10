use std::sync::Arc;

use crate::witness::chain_source::ChainStream;
use futures::StreamExt;
use pallet_cf_chain_tracking::ChainState;
use state_chain_runtime::PalletInstanceAlias;

use crate::{
	state_chain_observer::client::extrinsic_api::signed::SignedExtrinsicApi,
	witness::common::{BoxActiveAndFuture, RuntimeCallHasChain, RuntimeHasChain},
};

use super::{ChunkedByTime, Item};

#[async_trait::async_trait]
pub trait GetTrackedData<C: cf_chains::Chain>: Send + Sync + Clone {
	async fn get_tracked_data(&self, block_number: C::ChainBlockNumber) -> C::TrackedData;
}

pub struct ChainTracking<Inner, StateChainClient, TrackedDataClient> {
	inner: Inner,
	tracked_data_client: TrackedDataClient,
	state_chain_client: Arc<StateChainClient>,
}

impl<Inner, StateChainClient, TrackedDataClient>
	ChainTracking<Inner, StateChainClient, TrackedDataClient>
{
	pub fn new(
		inner: Inner,
		state_chain_client: Arc<StateChainClient>,
		tracked_data_client: TrackedDataClient,
	) -> ChainTracking<Inner, StateChainClient, TrackedDataClient> {
		ChainTracking { inner, state_chain_client, tracked_data_client }
	}
}

#[async_trait::async_trait]
impl<Inner, StateChainClient, TrackedDataClient> ChunkedByTime
	for ChainTracking<Inner, StateChainClient, TrackedDataClient>
where
	Inner: ChunkedByTime,
	StateChainClient: SignedExtrinsicApi + Send + Sync + 'static,
	TrackedDataClient: GetTrackedData<Inner::Chain>,
	state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
	state_chain_runtime::RuntimeCall:
		RuntimeCallHasChain<state_chain_runtime::Runtime, Inner::Chain>,
{
	type Index = Inner::Index;
	type Hash = Inner::Hash;
	type Data = Inner::Data;

	type Client = Inner::Client;

	type Chain = Inner::Chain;

	type Parameters = Inner::Parameters;

	async fn stream(
		&self,
		parameters: Inner::Parameters,
	) -> BoxActiveAndFuture<'_, Item<'_, Self>> {
		let state_chain_client = self.state_chain_client.clone();
		let tracked_data_client = self.tracked_data_client.clone();

		self.inner
			.stream(parameters)
			.await
			.filter(|(epoch, _, _)| {
				futures::future::ready(epoch.historic_signal.clone().get().is_none())
			})
			.await
			.then(move |(epoch, chain_stream, chain_client)| {
				let state_chain_client = state_chain_client.clone();
				let tracked_data_client = tracked_data_client.clone();
				async move {
					(
						epoch.clone(),
						chain_stream
							.then(move |header| {
								let state_chain_client = state_chain_client.clone();
								let tracked_data_client = tracked_data_client.clone();
								async move {
									// Unclear error when this is inlined "error: higher-ranked
									// lifetime error"
									let call: Box<state_chain_runtime::RuntimeCall> = Box::new(
										pallet_cf_chain_tracking::Call::<
											state_chain_runtime::Runtime,
											<Inner::Chain as PalletInstanceAlias>::Instance,
										>::update_chain_state {
											new_chain_state: ChainState {
												block_height: header.index,
												tracked_data: tracked_data_client
													.get_tracked_data(header.index)
													.await,
											},
										}
										.into(),
									);
									state_chain_client
										.submit_signed_extrinsic(
											pallet_cf_witnesser::Call::witness_at_epoch {
												call,
												epoch_index: epoch.index,
											},
										)
										.await;
									header
								}
							})
							.into_box(),
						chain_client,
					)
				}
			})
			.await
			.into_box()
	}
}
