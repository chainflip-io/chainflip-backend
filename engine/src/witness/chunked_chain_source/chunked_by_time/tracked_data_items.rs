use std::sync::Arc;

use crate::witness::chain_source::{ChainClient, ChainStream};
use frame_support::CloneNoBound;
use futures_core::FusedStream;
use futures_util::{stream, StreamExt};
use state_chain_runtime::PalletInstanceAlias;
use utilities::{loop_select, task_scope::Scope, UnendingStream};

use crate::{
	state_chain_observer::client::{storage_api::StorageApi, StateChainStreamApi},
	witness::{
		chain_source::Header,
		common::{RuntimeHasChain, STATE_CHAIN_CONNECTION},
	},
};

use super::{builder::ChunkedByTimeBuilder, ChunkedByTime};
use pallet_cf_chain_tracking::ChainState;

/// Gives us the latest tracked data we know of as part of the header data.
/// Allowing us to compare our current query of the tracked data to potentially
/// change what we do/submit within the chain tracking.
#[allow(clippy::type_complexity)]
pub struct TrackedDataItems<Inner: ChunkedByTime>
where
	state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
{
	inner: Inner,
	receiver: tokio::sync::watch::Receiver<Option<ChainState<Inner::Chain>>>,
}

impl<Inner: ChunkedByTime> TrackedDataItems<Inner>
where
	state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
{
	pub async fn get_tracked_data<StateChainClient: StorageApi + Send + Sync + 'static>(
		state_chain_client: &StateChainClient,
		block_hash: state_chain_runtime::Hash,
	) -> Option<ChainState<Inner::Chain>>
	where
		state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
	{
		state_chain_client
			.storage_value::<pallet_cf_chain_tracking::CurrentChainState<
				state_chain_runtime::Runtime,
				<Inner::Chain as PalletInstanceAlias>::Instance,
			>>(block_hash)
			.await
			.expect(STATE_CHAIN_CONNECTION)
	}

	pub async fn new<
		'env,
		StateChainStream: StateChainStreamApi,
		StateChainClient: StorageApi + Send + Sync + 'static,
	>(
		inner: Inner,
		scope: &Scope<'env, anyhow::Error>,
		mut state_chain_stream: StateChainStream,
		state_chain_client: Arc<StateChainClient>,
	) -> Self {
		let (sender, receiver) = tokio::sync::watch::channel(
			Self::get_tracked_data(&*state_chain_client, state_chain_stream.cache().block_hash)
				.await,
		);

		scope.spawn(async move {
			utilities::loop_select! {
				let _ = sender.closed() => { break Ok(()) },
				if let Some((_block_hash, _block_header)) = state_chain_stream.next() => {
					let _result = sender.send(Self::get_tracked_data(&*state_chain_client, _block_hash).await);
				} else break Ok(()),
			}
		});

		Self { inner, receiver }
	}
}

#[derive(CloneNoBound)]
pub struct TrackedDataItemsClient<Inner: ChunkedByTime>
where
	state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
{
	inner_client: Inner::Client,
	receiver: tokio::sync::watch::Receiver<Option<ChainState<Inner::Chain>>>,
}
impl<Inner: ChunkedByTime> TrackedDataItemsClient<Inner>
where
	state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
{
	pub fn new(
		inner_client: Inner::Client,
		receiver: tokio::sync::watch::Receiver<Option<ChainState<Inner::Chain>>>,
	) -> Self {
		Self { inner_client, receiver }
	}
}

#[async_trait::async_trait]
impl<Inner: ChunkedByTime> ChunkedByTime for TrackedDataItems<Inner>
where
	state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
{
	type Index = Inner::Index;
	type Hash = Inner::Hash;
	type Data = (Inner::Data, Option<ChainState<Inner::Chain>>);

	type Client = TrackedDataItemsClient<Inner>;

	type Chain = Inner::Chain;

	type Parameters = Inner::Parameters;

	async fn stream(
		&self,
		parameters: Self::Parameters,
	) -> crate::witness::common::BoxActiveAndFuture<'_, super::Item<'_, Self>> {
		self.inner
			.stream(parameters)
			.await
			.then(move |(epoch, chain_stream, chain_client)| async move {
				(
					epoch,
					stream::unfold(
						(chain_stream.fuse(), self.receiver.clone()),
						|(mut chain_stream, receiver)| async move {
							loop_select!(
								if chain_stream.is_terminated() => break None,

								// TODO: Loop through the stream until we get to the end
								let header = chain_stream.next_or_pending() => {
									// Always get the latest chain state
									let tracked_data = receiver.borrow().clone();
									break Some((header.map_data(|header| (header.data, tracked_data)), (chain_stream, receiver)))
								},
							)
						},
					)
					.into_box(),
					TrackedDataItemsClient::new(chain_client, self.receiver.clone()),
				)
			})
			.await
			.into_box()
	}
}

#[async_trait::async_trait]
impl<Inner: ChunkedByTime> ChainClient for TrackedDataItemsClient<Inner>
where
	state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
{
	type Index = Inner::Index;
	type Hash = Inner::Hash;
	type Data = (Inner::Data, Option<ChainState<Inner::Chain>>);

	async fn header_at_index(
		&self,
		index: Self::Index,
	) -> Header<Self::Index, Self::Hash, Self::Data> {
		let tracked_data_items = self.receiver.borrow().clone();
		self.inner_client
			.header_at_index(index)
			.await
			.map_data(|header| (header.data, tracked_data_items))
	}
}

impl<Inner: ChunkedByTime> ChunkedByTimeBuilder<Inner> {
	pub async fn tracked_data_items<'env, StateChainStream, StateChainClient>(
		self,
		scope: &Scope<'env, anyhow::Error>,
		state_chain_stream: StateChainStream,
		state_chain_client: Arc<StateChainClient>,
	) -> ChunkedByTimeBuilder<TrackedDataItems<Inner>>
	where
		state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
		StateChainStream: StateChainStreamApi,
		StateChainClient: StorageApi + Send + Sync + 'static,
	{
		ChunkedByTimeBuilder::new(
			TrackedDataItems::new(self.source, scope, state_chain_stream, state_chain_client).await,
			self.parameters,
		)
	}
}
