use std::{marker::PhantomData, sync::Arc};

use futures::StreamExt;

use crate::state_chain_observer::client::extrinsic_api::signed::SignedExtrinsicApi;

use super::{
	chain_source::box_chain_stream,
	common::BoxActiveAndFuture,
	split_by_epoch::{ChainSplitByEpoch, Item},
};

#[async_trait::async_trait]
pub trait GetChainTrackingData<C: cf_chains::Chain> {
	async fn get_chain_tracking_data(&self) -> C::TrackedData;
}

pub struct ChainTracking<'a, C, I, InnerStream, StateChainClient, Client>
where
	InnerStream: ChainSplitByEpoch<'a>,
	C: cf_chains::Chain,
	I: 'static + Send + Sync,
	Client: GetChainTrackingData<C>,
{
	inner_stream: InnerStream,
	client: Client,
	state_chain_client: Arc<StateChainClient>,
	phantom: PhantomData<(&'a (), C, I)>,
}

impl<'a, C, I, InnerStream, StateChainClient, Client>
	ChainTracking<'a, C, I, InnerStream, StateChainClient, Client>
where
	C: cf_chains::Chain,
	I: 'static + Send + Sync,
	Client: GetChainTrackingData<C>,
	InnerStream: ChainSplitByEpoch<'a>,
	StateChainClient: SignedExtrinsicApi + Send,
{
	pub fn new(
		inner_stream: InnerStream,
		state_chain_client: Arc<StateChainClient>,
		client: Client,
	) -> ChainTracking<'a, C, I, InnerStream, StateChainClient, Client>
	where
		InnerStream: ChainSplitByEpoch<'a>,
		Client: GetChainTrackingData<C>,
	{
		ChainTracking { inner_stream, state_chain_client, client, phantom: PhantomData }
	}
}

#[async_trait::async_trait]
impl<'a, C, I, InnerStream, StateChainClient, Client> ChainSplitByEpoch<'a>
	for ChainTracking<'a, C, I, InnerStream, StateChainClient, Client>
where
	C: cf_chains::Chain,
	I: 'static + Send + Sync,
	InnerStream: ChainSplitByEpoch<'a>,
	Client: GetChainTrackingData<C> + Send + Sync + Clone + 'static,
	StateChainClient: SignedExtrinsicApi + Send + Sync + 'static,
	state_chain_runtime::Runtime: pallet_cf_chain_tracking::Config<I, TargetChain = C>,
	state_chain_runtime::RuntimeCall:
		std::convert::From<pallet_cf_chain_tracking::Call<state_chain_runtime::Runtime, I>>,
{
	type UnderlyingChainSource = InnerStream::UnderlyingChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::UnderlyingChainSource>> {
		let state_chain_client = self.state_chain_client.clone();
		let client = self.client.clone();

		self.inner_stream
			.stream()
			.await
			.filter(|(epoch, _)| {
				futures::future::ready(epoch.historic_signal.clone().get().is_some())
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
								let chain_tracking_data = client.get_chain_tracking_data().await;
								state_chain_client
									.submit_signed_extrinsic(
										pallet_cf_witnesser::Call::witness_at_epoch {
											call: Box::new(
												pallet_cf_chain_tracking::Call::<
													state_chain_runtime::Runtime,
													I,
												>::update_chain_state {
													state: chain_tracking_data,
												}
												.into(),
											),
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
