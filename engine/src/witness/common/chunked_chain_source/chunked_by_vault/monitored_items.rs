use std::sync::Arc;

use cf_chains::ChainState;
use frame_support::CloneNoBound;
use futures::{Future, FutureExt};
use futures_util::{stream, StreamExt};
use state_chain_runtime::PalletInstanceAlias;
use tokio::sync::watch;
use utilities::{
	loop_select,
	task_scope::{Scope, OR_CANCEL},
};

use crate::{
	state_chain_observer::client::{storage_api::StorageApi, StateChainStreamApi},
	witness::common::{
		chain_source::{ChainClient, ChainStream, Header},
		RuntimeHasChain, STATE_CHAIN_BEHAVIOUR, STATE_CHAIN_CONNECTION,
	},
};

use super::{builder::ChunkedByVaultBuilder, ChunkedByVault};

/// This helps ensure the set of ingress addresses witnessed at each block are consistent across
/// every validator
fn is_header_ready<Inner: ChunkedByVault>(
	index: Inner::Index,
	chain_state: &ChainState<Inner::Chain>,
) -> bool {
	index < chain_state.block_height
}

// We need to pass in something that can act as the generic `addresses_for_header`
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
		ItemGetterGenerator,
	>(
		state_chain_client: &StateChainClient,
		block_hash: state_chain_runtime::Hash,
		get_items: ItemGetterGenerator,
	) -> (ChainState<Inner::Chain>, MonitoredItems)
	where
		state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
		GetItemsFut: Future<Output = MonitoredItems> + Send + 'static,
		ItemGetterGenerator:
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
		StateChainStream: StateChainStreamApi<FINALIZED>,
		StateChainClient: StorageApi + Send + Sync + 'static,
		GetItemsFut: Future<Output = MonitoredItems> + Send + 'static,
		ItemGetterGenerator: Fn(state_chain_runtime::Hash) -> GetItemsFut + Send + Sync + Clone + 'static,
		const FINALIZED: bool,
	>(
		inner: Inner,
		scope: &Scope<'env, anyhow::Error>,
		mut state_chain_stream: StateChainStream,
		state_chain_client: Arc<StateChainClient>,
		filter_fn: ItemFilter,
		get_items: ItemGetterGenerator,
	) -> Self
	where
		state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
	{
		let (sender, receiver) = watch::channel(
			Self::get_chain_state_and_items(
				&*state_chain_client,
				state_chain_stream.cache().hash,
				get_items.clone(),
			)
			.await,
		);

		let get_items_c = get_items.clone();
		scope.spawn(async move {
			utilities::loop_select! {
				let _ = sender.closed() => { break Ok(()) },
				if let Some(_block_header) = state_chain_stream.next() => {
					// Note it is still possible for engines to inconsistently select addresses to witness for a
					// block due to how the SC expiries deposit addresses
				let _result = sender.send(Self::get_chain_state_and_items(&*state_chain_client, state_chain_stream.cache().hash, get_items_c.clone()).await);
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
								// We're saying the block itself is ready. But the addresses within
								// that block.
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
															if !state.ready_headers.is_empty() => break Some((state.ready_headers.pop().unwrap(),
							(chain_stream, state))), 								if let Some(header) = chain_stream.next() => {
																state.add_headers(std::iter::once(header));
															} else disable then if state.pending_headers.is_empty() => break None,
															let _ = state.receiver.changed().map(|result| result.expect(OR_CANCEL)) => {
																// headers we weren't yet ready to process, but they might be ready now if the chaint tracking
							// has changed

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
				.await
				.expect(OR_CANCEL);
			let (_option_chain_state, addresses) = &*chain_state_and_addresses;

			(self.filter_fn)(index, addresses)
		};

		self.inner_client
			.header_at_index(index)
			.await
			.map_data(|header| (header.data, addresses))
	}
}

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	pub async fn monitored_sc_items<
		'env,
		StateChainStream,
		StateChainClient,
		MonitoredItems,
		ItemFilter,
		GetItemsFut,
		ItemGetterGenerator,
		const FINALIZED: bool,
	>(
		self,
		scope: &Scope<'env, anyhow::Error>,
		state_chain_stream: StateChainStream,
		state_chain_client: Arc<StateChainClient>,
		filter_fn: ItemFilter,
		get_items: ItemGetterGenerator,
	) -> ChunkedByVaultBuilder<MonitoredSCItems<Inner, MonitoredItems, ItemFilter>>
	where
		state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
		StateChainStream: StateChainStreamApi<FINALIZED>,
		StateChainClient: StorageApi + Send + Sync + 'static,
		MonitoredItems: Send + Sync + Unpin + 'static,
		ItemFilter:
			Fn(Inner::Index, &MonitoredItems) -> MonitoredItems + Send + Sync + Clone + 'static,
		GetItemsFut: Future<Output = MonitoredItems> + Send + 'static,
		ItemGetterGenerator:
			Fn(state_chain_runtime::Hash) -> GetItemsFut + Send + Sync + Clone + 'static,
	{
		ChunkedByVaultBuilder::new(
			MonitoredSCItems::new(
				self.source,
				scope,
				state_chain_stream,
				state_chain_client,
				filter_fn,
				get_items,
			)
			.await,
			self.parameters,
		)
	}
}
