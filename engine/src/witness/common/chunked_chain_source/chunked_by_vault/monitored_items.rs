use std::sync::Arc;

use cf_chains::ChainState;
use frame_support::CloneNoBound;
use futures::Future;
use futures_util::{stream, StreamExt};
use state_chain_runtime::PalletInstanceAlias;
use tokio::sync::watch;
use utilities::{
	loop_select,
	task_scope::{Scope, UnwrapOrCancel},
};

use crate::{
	state_chain_observer::client::{
		storage_api::StorageApi, stream_api::StreamApi, STATE_CHAIN_BEHAVIOUR,
		STATE_CHAIN_CONNECTION,
	},
	witness::common::{
		chain_source::{ChainClient, ChainStream, Header},
		RuntimeHasChain,
	},
};

use super::ChunkedByVault;

/// This helps ensure the set of ingress addresses witnessed at each block are consistent across
/// every validator. We only consider a header ready when the chain tracking has passed the block.
/// This gives the CFEs a single point to synchronise against.
fn is_header_ready<Inner: ChunkedByVault>(
	index: Inner::Index,
	chain_state: &ChainState<Inner::Chain>,
) -> bool {
	index < chain_state.block_height
}

/// This helps ensure a set of items we want to witness are consistent for each block across all
/// validators. Without this functionality of holding up blocks and filtering out items, the
/// CFEs can go out of sync. Consider the case of 2 CFEs.
/// - CFE A is ahead of CFE B with respect to external chain X by 1 block.
/// - CFE B witnesses block 10 of X, and is watching for addresses that it fetches from the SC, at
///   SC block 50.
/// - CFE A witnesses block 11 of X, and is watching for addresses that it fetches from the SC, at
///   the same SC block 50.
/// - The SC progresses to block 51, revealing that an address is to be witnessed.
/// - There is a deposit at block 10 of X, which CFE B witnesses, but CFE A does not.
/// If CFE A does not wait until the block is ready to process it can miss witnesses and be out
/// of sync with the other CFEs.
#[derive(Clone)]
#[allow(clippy::type_complexity)]
pub struct MonitoredSCItems<Inner: ChunkedByVault, MonitoredItems, ItemFilter>
where
	state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
	MonitoredItems: Send + Sync + 'static,
	ItemFilter: Fn(Inner::Index, &MonitoredItems) -> MonitoredItems + Send + Sync + Clone + 'static,
{
	inner: Inner,
	receiver: tokio::sync::watch::Receiver<(ChainState<Inner::Chain>, MonitoredItems)>,
	filter_fn: ItemFilter,
}

impl<Inner: ChunkedByVault, MonitoredItems, ItemFilter>
	MonitoredSCItems<Inner, MonitoredItems, ItemFilter>
where
	state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
	MonitoredItems: Send + Sync + 'static,
	ItemFilter: Fn(Inner::Index, &MonitoredItems) -> MonitoredItems + Send + Sync + Clone + 'static,
{
	async fn get_chain_state_and_items<
		StateChainClient: StorageApi + Send + Sync + 'static,
		GetItemsFut,
		GetItemsGenerator,
	>(
		state_chain_client: &StateChainClient,
		block_hash: state_chain_runtime::Hash,
		get_items: &GetItemsGenerator,
	) -> (ChainState<Inner::Chain>, MonitoredItems)
	where
		state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
		GetItemsFut: Future<Output = MonitoredItems> + Send + 'static,
		GetItemsGenerator:
			Fn(state_chain_runtime::Hash) -> GetItemsFut + Send + Sync + Clone + 'static,
	{
		(
			state_chain_client
				.storage_value::<pallet_cf_chain_tracking::CurrentChainState<
					state_chain_runtime::Runtime,
					<Inner::Chain as PalletInstanceAlias>::Instance,
				>>(block_hash)
				.await
				.expect(STATE_CHAIN_CONNECTION)
				.expect(STATE_CHAIN_BEHAVIOUR),
			get_items(block_hash).await,
		)
	}

	pub async fn new<
		'env,
		StateChainStream: StreamApi<IS_FINALIZED>,
		StateChainClient: StorageApi + Send + Sync + 'static,
		GetItemsFut: Future<Output = MonitoredItems> + Send + 'static,
		GetItemsGenerator: Fn(state_chain_runtime::Hash) -> GetItemsFut + Send + Sync + Clone + 'static,
		const IS_FINALIZED: bool,
	>(
		inner: Inner,
		scope: &Scope<'env, anyhow::Error>,
		mut state_chain_stream: StateChainStream,
		state_chain_client: Arc<StateChainClient>,
		get_items: GetItemsGenerator,
		filter_fn: ItemFilter,
	) -> Self
	where
		state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
	{
		let (sender, receiver) = watch::channel(
			Self::get_chain_state_and_items(
				&*state_chain_client,
				state_chain_stream.cache().hash,
				&get_items,
			)
			.await,
		);

		scope.spawn(async move {
			utilities::loop_select! {
				let _ = sender.closed() => { break Ok(()) },
				if let Some(_block_header) = state_chain_stream.next() => {
					// Note it is still possible for engines to inconsistently select addresses to witness for a
					// block due to how the SC expiries deposit addresses
				let _result = sender.send(Self::get_chain_state_and_items(&*state_chain_client, state_chain_stream.cache().hash, &get_items).await);
				} else break Ok(()),
			}
		});

		Self { inner, receiver, filter_fn }
	}
}
#[async_trait::async_trait]
impl<Inner: ChunkedByVault, MonitoredItems, ItemFilter> ChunkedByVault
	for MonitoredSCItems<Inner, MonitoredItems, ItemFilter>
where
	state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
	MonitoredItems: Send + Sync + Unpin + 'static,
	ItemFilter: Fn(Inner::Index, &MonitoredItems) -> MonitoredItems + Send + Sync + Clone + 'static,
{
	type ExtraInfo = Inner::ExtraInfo;
	type ExtraHistoricInfo = Inner::ExtraHistoricInfo;

	type Index = Inner::Index;
	type Hash = Inner::Hash;

	type Data = (Inner::Data, MonitoredItems);

	type Client = MonitoredSCItemsClient<Inner, MonitoredItems, ItemFilter>;

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
				struct State<Inner: ChunkedByVault, MonitoredItems, ItemFilter>
				where
					state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
					MonitoredItems: Send + Sync + 'static,
					ItemFilter: Fn(Inner::Index, &MonitoredItems) -> MonitoredItems
						+ Send
						+ Sync
						+ Clone
						+ 'static,
				{
					receiver:
						tokio::sync::watch::Receiver<(ChainState<Inner::Chain>, MonitoredItems)>,
					pending_headers: Vec<Header<Inner::Index, Inner::Hash, Inner::Data>>,
					ready_headers:
						Vec<Header<Inner::Index, Inner::Hash, (Inner::Data, MonitoredItems)>>,
					filter_fn: ItemFilter,
				}
				impl<Inner: ChunkedByVault, MonitoredItems, ItemFilter> State<Inner, MonitoredItems, ItemFilter>
				where
					state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
					MonitoredItems: Send + Sync + 'static,
					ItemFilter: Fn(Inner::Index, &MonitoredItems) -> MonitoredItems
						+ Send
						+ Sync
						+ Clone
						+ 'static,
				{
					fn add_headers<
						It: IntoIterator<Item = Header<Inner::Index, Inner::Hash, Inner::Data>>,
					>(
						&mut self,
						headers: It,
					) {
						let chain_state_and_addresses = self.receiver.borrow();
						let (chain_state, addresses) = &*chain_state_and_addresses;
						for header in headers {
							if is_header_ready::<Inner>(header.index, chain_state) {
								// We're saying the block itself is ready. But the items within that header may not be required.
								// Consider the cases:
								// 1. An item in the header has expired. Its expiry block is after the current block.
								// 2. An item in the header has an initiation/starting block after the current block.
								self.ready_headers.push(header.map_data(|header| {
									(header.data, (self.filter_fn)(header.index, addresses))
								}));
							} else {
								self.pending_headers.push(header);
							}
						}
					}
				}

				(
					epoch,
					stream::unfold(
						(
							chain_stream.fuse(),
							State::<Inner, MonitoredItems, ItemFilter> {
								receiver: self.receiver.clone(),
								pending_headers: vec![],
								ready_headers: vec![],
								filter_fn: self.filter_fn.clone(),
							},
						),
						|(mut chain_stream, mut state)| async move {
							loop_select!(
								if !state.ready_headers.is_empty() => break Some((state.ready_headers.pop().unwrap(), (chain_stream, state))),
								if let Some(header) = chain_stream.next() => {
									state.add_headers(std::iter::once(header));
								} else disable then if state.pending_headers.is_empty() => break None,
								let _ = state.receiver.changed().unwrap_or_cancel() => {
									// Headers we weren't yet ready to process might be ready now if the chain tracking has progressed.
									let pending_headers = std::mem::take(&mut state.pending_headers);
									state.add_headers(pending_headers);
								},
							)
						},
					)
					.into_box(),
					MonitoredSCItemsClient::new(
						chain_client,
						self.receiver.clone(),
						(self.filter_fn).clone(),
					),
				)
			})
			.await
			.into_box()
	}
}

#[derive(CloneNoBound)]
pub struct MonitoredSCItemsClient<Inner: ChunkedByVault, MonitoredItems, ItemFilter>
where
	state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
	ItemFilter: Fn(Inner::Index, &MonitoredItems) -> MonitoredItems + Send + Sync + Clone + 'static,
{
	inner_client: Inner::Client,
	receiver: tokio::sync::watch::Receiver<(ChainState<Inner::Chain>, MonitoredItems)>,
	filter_fn: ItemFilter,
}

impl<Inner: ChunkedByVault, MonitoredItems, ItemFilter>
	MonitoredSCItemsClient<Inner, MonitoredItems, ItemFilter>
where
	state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
	ItemFilter: Fn(Inner::Index, &MonitoredItems) -> MonitoredItems + Send + Sync + Clone + 'static,
{
	pub fn new(
		inner_client: Inner::Client,
		receiver: tokio::sync::watch::Receiver<(ChainState<Inner::Chain>, MonitoredItems)>,
		filter_fn: ItemFilter,
	) -> Self {
		Self { inner_client, receiver, filter_fn }
	}
}
#[async_trait::async_trait]
impl<Inner: ChunkedByVault, MonitoredItems, ItemFilter> ChainClient
	for MonitoredSCItemsClient<Inner, MonitoredItems, ItemFilter>
where
	state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
	MonitoredItems: Send + Sync + Unpin + 'static,
	ItemFilter: Fn(Inner::Index, &MonitoredItems) -> MonitoredItems + Send + Sync + Clone + 'static,
{
	type Index = Inner::Index;
	type Hash = Inner::Hash;
	type Data = (Inner::Data, MonitoredItems);

	async fn header_at_index(
		&self,
		index: Self::Index,
	) -> Header<Self::Index, Self::Hash, Self::Data> {
		let mut receiver = self.receiver.clone();

		let addresses = {
			let chain_state_and_addresses = receiver
				.wait_for(|(chain_state, _addresses)| is_header_ready::<Inner>(index, chain_state))
				.unwrap_or_cancel()
				.await;
			let (_option_chain_state, addresses) = &*chain_state_and_addresses;

			(self.filter_fn)(index, addresses)
		};

		self.inner_client
			.header_at_index(index)
			.await
			.map_data(|header| (header.data, addresses))
	}
}
