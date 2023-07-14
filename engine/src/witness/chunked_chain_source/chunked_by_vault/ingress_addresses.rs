// sc stream
// look at chain tracking
// pace
// keep record of ingress addresses
// once chain tracking passes block index, give out list of ingress addresses (clone, hash?)

use std::sync::Arc;

use crate::witness::{
	chain_source::{ChainClient, ChainStream},
	chunked_chain_source::Builder,
};
use cf_chains::Chain;
use futures::FutureExt;
use futures_core::FusedStream;
use futures_util::{
	stream::{self, Fuse},
	StreamExt,
};
use pallet_cf_ingress_egress::DepositAddressDetails;
use state_chain_runtime::PalletInstanceAlias;
use tokio::sync::watch;
use utilities::{
	loop_select,
	task_scope::{Scope, OR_CANCEL},
	UnendingStream,
};

use crate::{
	state_chain_observer::client::{storage_api::StorageApi, StateChainStreamApi},
	witness::{
		chain_source::{BoxChainStream, Header},
		common::{RuntimeHasChain, STATE_CHAIN_CONNECTION},
	},
};

use super::{ChunkedByVault, ChunkedByVaultAlias, Generic};

#[allow(clippy::type_complexity)]
pub struct IngressAddresses<Inner: ChunkedByVault> {
	inner: Inner,
	receiver: tokio::sync::watch::Receiver<(
		Option<pallet_cf_chain_tracking::ChainState<Inner::Chain>>,
		Vec<(<Inner::Chain as Chain>::ChainAccount, DepositAddressDetails<Inner::Chain>)>,
	)>,
}
impl<Inner: ChunkedByVault> IngressAddresses<Inner> {
	pub async fn new<
		'env,
		StateChainStream: StateChainStreamApi,
		StateChainClient: StorageApi + Send + Sync + 'static,
	>(
		inner: Inner,
		scope: &Scope<'env, anyhow::Error>,
		mut state_chain_stream: StateChainStream,
		state_chain_client: Arc<StateChainClient>,
	) -> Self
	where
		state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
	{
		let (sender, receiver) = watch::channel((
			state_chain_client
				.storage_value::<pallet_cf_chain_tracking::CurrentChainState<
					state_chain_runtime::Runtime,
					<Inner::Chain as PalletInstanceAlias>::Instance,
				>>(state_chain_stream.cache().block_hash)
				.await
				.expect(STATE_CHAIN_CONNECTION),
			state_chain_client
				.storage_map::<pallet_cf_ingress_egress::DepositAddressDetailsLookup<
					state_chain_runtime::Runtime,
					<Inner::Chain as PalletInstanceAlias>::Instance,
				>>(state_chain_stream.cache().block_hash)
				.await
				.expect(STATE_CHAIN_CONNECTION),
		));

		scope.spawn(async move {
            utilities::loop_select! {
                let _ = sender.closed() => { break Ok(()) },
                if let Some((_block_hash, _block_header)) = state_chain_stream.next() => {
                    let _result = sender.send((
                        state_chain_client.storage_value::<pallet_cf_chain_tracking::CurrentChainState<state_chain_runtime::Runtime, <Inner::Chain as PalletInstanceAlias>::Instance>>(state_chain_stream.cache().block_hash).await.expect(STATE_CHAIN_CONNECTION),
                        state_chain_client
                            .storage_map::<pallet_cf_ingress_egress::DepositAddressDetailsLookup<
                                state_chain_runtime::Runtime,
                                <Inner::Chain as PalletInstanceAlias>::Instance,
                            >>(state_chain_stream.cache().block_hash)
                            .await
							.expect(STATE_CHAIN_CONNECTION)
                        ));
                } else break Ok(()),
            }
        });

		Self { inner, receiver }
	}
}
#[async_trait::async_trait]
impl<Inner: ChunkedByVault> ChunkedByVault for IngressAddresses<Inner> {
	type Index = Inner::Index;
	type Hash = Inner::Hash;
	type Data = (
		Inner::Data,
		Vec<(<Inner::Chain as Chain>::ChainAccount, DepositAddressDetails<Inner::Chain>)>,
	);

	type Client = IngressAddressesClient<Inner>;

	type Chain = Inner::Chain;

	type Parameters = Inner::Parameters;

	async fn stream(
		&self,
		parameters: Self::Parameters,
	) -> crate::witness::common::BoxActiveAndFuture<'_, super::Item<'_, Self>> {
		self.inner.stream(parameters).await.then(move |(epoch, chain_stream, chain_client)| async move {

			type ChainState<Inner> = pallet_cf_chain_tracking::ChainState<<Inner as ChunkedByVault>::Chain>;

			type Addresses<Inner> = Vec<(
				<<Inner as ChunkedByVault>::Chain as Chain>::ChainAccount,
				DepositAddressDetails<<Inner as ChunkedByVault>::Chain>,
			)>;

			struct State<'a, Inner: ChunkedByVault> {
				receiver: tokio::sync::watch::Receiver<(Option<ChainState<Inner>>, Addresses<Inner>)>,
				chain_stream: Fuse<BoxChainStream<'a, Inner::Index, Inner::Hash, Inner::Data>>,
				pending_headers: Vec<Header<Inner::Index, Inner::Hash, Inner::Data>>,
				ready_headers: Vec<Header<Inner::Index, Inner::Hash, (
					Inner::Data,
					Vec<(<Inner::Chain as Chain>::ChainAccount, DepositAddressDetails<Inner::Chain>)>,
				)>>,
			}

			(
				epoch,
				stream::unfold(State::<'_, Inner> {
					receiver: self.receiver.clone(),
					chain_stream: chain_stream.fuse(),
					pending_headers: vec![],
					ready_headers: vec![]
				}, |mut state| async move {
					if let Some(header) = state.ready_headers.pop() {
						Some((header, state))
					} else {
						loop_select!(
							if state.chain_stream.is_terminated() && state.pending_headers.is_empty() => break None,
							let header = state.chain_stream.next_or_pending() => {
								if let Some(header) = {
									let chain_state_and_addresses = state.receiver.borrow();
									let (option_chain_state, addresses) = &*chain_state_and_addresses;
									if option_chain_state.as_ref().is_some_and(|chain_state| header.index <= chain_state.block_height) {
										let addresses = addresses.iter().filter(|(_, details)| details.opened_at <= header.index).cloned().collect();
										Some(header.map(|data| (data, addresses)))
									} else {
										state.pending_headers.push(header);
										None
									}
								} {
									break Some((header, state))
								}
							},
							let _ = state.receiver.changed().map(|result| result.expect(OR_CANCEL)) => {
								let chain_state_and_addresses = state.receiver.borrow();
								let (option_chain_state, addresses) = &*chain_state_and_addresses;
								if let Some(chain_state) = option_chain_state {
									let mut new_ready_headers = state.pending_headers.drain_filter(|header| header.index <= chain_state.block_height).map(|header| header.map(|data| (data, addresses.iter().filter(|(_, details)| details.opened_at <= chain_state.block_height).cloned().collect())));
									if let Some(header) = new_ready_headers.next() {
										state.ready_headers.extend(new_ready_headers);
										drop(chain_state_and_addresses);
										break Some((header, state))
									}
								}
							},
						)
					}
				}).into_box(),
				IngressAddressesClient::new(chain_client, self.receiver.clone())
			)
		}).await.into_box()
	}
}

type Addresses<Inner> = Vec<(
	<<Inner as ChunkedByVault>::Chain as Chain>::ChainAccount,
	DepositAddressDetails<<Inner as ChunkedByVault>::Chain>,
)>;

pub struct IngressAddressesClient<Inner: ChunkedByVault> {
	inner_client: Inner::Client,
	receiver: tokio::sync::watch::Receiver<(
		Option<pallet_cf_chain_tracking::ChainState<Inner::Chain>>,
		Addresses<Inner>,
	)>,
}
impl<Inner: ChunkedByVault> IngressAddressesClient<Inner> {
	pub fn new(
		inner_client: Inner::Client,
		receiver: tokio::sync::watch::Receiver<(
			Option<pallet_cf_chain_tracking::ChainState<Inner::Chain>>,
			Addresses<Inner>,
		)>,
	) -> Self {
		Self { inner_client, receiver }
	}
}
#[async_trait::async_trait]
impl<Inner: ChunkedByVault> ChainClient for IngressAddressesClient<Inner> {
	type Index = Inner::Index;
	type Hash = Inner::Hash;
	type Data = (
		Inner::Data,
		Vec<(<Inner::Chain as Chain>::ChainAccount, DepositAddressDetails<Inner::Chain>)>,
	);

	async fn header_at_index(
		&self,
		index: Self::Index,
	) -> Header<Self::Index, Self::Hash, Self::Data> {
		let mut receiver = self.receiver.clone();

		let addresses = {
			let chain_state_and_addresses = receiver
				.wait_for(|(option_chain_state, _addresses)| {
					option_chain_state
						.as_ref()
						.is_some_and(|chain_state| index <= chain_state.block_height)
				})
				.await
				.expect(OR_CANCEL);
			let (_option_chain_state, addresses) = &*chain_state_and_addresses;
			addresses
				.iter()
				.filter(|(_, details)| details.opened_at <= index)
				.cloned()
				.collect()
		};

		self.inner_client.header_at_index(index).await.map(|data| (data, addresses))
	}
}

impl<Inner: ChunkedByVault> Builder<Generic<Inner>> {
	pub async fn ingress_addresses<'env, StateChainStream, StateChainClient>(
		self,
		scope: &Scope<'env, anyhow::Error>,
		state_chain_stream: StateChainStream,
		state_chain_client: Arc<StateChainClient>,
	) -> Builder<impl ChunkedByVaultAlias>
	where
		state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
		StateChainStream: StateChainStreamApi,
		StateChainClient: StorageApi + Send + Sync + 'static,
	{
		Builder {
			source: Generic(
				IngressAddresses::new(self.source, scope, state_chain_stream, state_chain_client)
					.await,
			),
			parameters: self.parameters,
		}
	}
}
