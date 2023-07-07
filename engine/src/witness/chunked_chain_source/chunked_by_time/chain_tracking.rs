/*
use std::{marker::PhantomData, sync::Arc};

use futures::StreamExt;
use pallet_cf_chain_tracking::ChainState;
use state_chain_runtime::PalletInstanceAlias;

use crate::state_chain_observer::client::extrinsic_api::signed::SignedExtrinsicApi;

use super::{
	chain_source::box_chain_stream,
	common::{BoxActiveAndFuture, ExternalChainSource, RuntimeCallHasChain, RuntimeHasChain},
	split_by_epoch::{ChunkedByTime, Item},
};

#[async_trait::async_trait]
pub trait GetTrackedData<C: cf_chains::Chain>: Send + Sync + Clone {
	async fn get_tracked_data(&self) -> ChainState<C>;
}

pub struct ChainTracking<'a, Inner, StateChainClient, TrackedDataClient> {
	inner_stream: Inner,
	client: TrackedDataClient,
	state_chain_client: Arc<StateChainClient>,
	_phantom: PhantomData<&'a ()>,
}

impl<'a, Inner, StateChainClient, TrackedDataClient>
	ChainTracking<'a, Inner, StateChainClient, TrackedDataClient>
{
	pub fn new(
		inner_stream: Inner,
		state_chain_client: Arc<StateChainClient>,
		client: TrackedDataClient,
	) -> ChainTracking<'a, Inner, StateChainClient, TrackedDataClient> {
		ChainTracking { inner_stream, state_chain_client, client, _phantom: PhantomData }
	}
}

#[async_trait::async_trait]
impl<'a, Inner, StateChainClient, TrackedDataClient> ChunkedByTime<'a>
	for ChainTracking<'a, Inner, StateChainClient, TrackedDataClient>
where
	Inner: ChunkedByTime<'a>,
	<Inner as ChunkedByTime<'a>>::InnerChainSource: ExternalChainSource,
	StateChainClient: SignedExtrinsicApi + Send + Sync + 'static,
	TrackedDataClient: GetTrackedData<<<Inner as ChunkedByTime<'a>>::InnerChainSource as ExternalChainSource>::Chain> + 'a,
	state_chain_runtime::Runtime: RuntimeHasChain<<<Inner as ChunkedByTime<'a>>::InnerChainSource as ExternalChainSource>::Chain>,
	state_chain_runtime::RuntimeCall: RuntimeCallHasChain<state_chain_runtime::Runtime, <<Inner as ChunkedByTime<'a>>::InnerChainSource as ExternalChainSource>::Chain>
{
	type InnerChainSource = Inner::InnerChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::InnerChainSource>> {
		let state_chain_client = self.state_chain_client.clone();
		let client = self.client.clone();

		self.inner_stream
			.stream()
			.await
			.filter(|(epoch, _)| {
				futures::future::ready(epoch.historic_signal.clone().get().is_none())
			})
			.await
			.then(move |(epoch, chain_stream)| {
				let state_chain_client = state_chain_client.clone();
				let client = client.clone();
				async move {
					(
						epoch.clone(),
						box_chain_stream(chain_stream.then(move |header| {
							let state_chain_client = state_chain_client.clone();
							let client = client.clone();
							async move {
								// Unclear error when this is inlined "error: higher-ranked lifetime error"
								let call: Box<state_chain_runtime::RuntimeCall> = Box::new(pallet_cf_chain_tracking::Call::<
										state_chain_runtime::Runtime,
										<<<Inner as ChunkedByTime<'a>>::InnerChainSource as ExternalChainSource>::Chain as PalletInstanceAlias>::Instance,
									>::update_chain_state {
										new_chain_state: client.get_tracked_data().await,
									}.into());
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
						})),
					)
				}
			})
			.await
			.into_box()
	}
}
*/
