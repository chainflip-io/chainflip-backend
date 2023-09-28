use std::sync::Arc;

use crate::witness::common::chain_source::{ChainClient, ChainStream};
use cf_chains::{Chain, ChainCrypto};
use frame_support::CloneNoBound;
use futures_core::FusedStream;
use futures_util::{stream, StreamExt};
use state_chain_runtime::PalletInstanceAlias;
use utilities::{loop_select, task_scope::Scope, UnendingStream};

use crate::{
	state_chain_observer::client::{storage_api::StorageApi, StateChainStreamApi},
	witness::common::{chain_source::Header, RuntimeHasChain, STATE_CHAIN_CONNECTION},
};

use super::{builder::ChunkedByVaultBuilder, ChunkedByVault};

pub type TxOutId<Inner> =
	<<<Inner as ChunkedByVault>::Chain as Chain>::ChainCrypto as ChainCrypto>::TransactionOutId;
pub type TxOutIds<Inner> = Vec<TxOutId<Inner>>;

/// This helps ensure the set of egress items witnessed at each block are consistent across
/// every validator.
/// The specific item monitored by each chain for determining what's an egress is different for each
/// chain. It's based on the TransactionOutId for each chain.
#[allow(clippy::type_complexity)]
pub struct EgressItems<Inner: ChunkedByVault>
where
	state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
{
	inner: Inner,
	receiver: tokio::sync::watch::Receiver<TxOutIds<Inner>>,
}

impl<Inner: ChunkedByVault> EgressItems<Inner>
where
	state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
{
	pub async fn get_transaction_out_ids<StateChainClient: StorageApi + Send + Sync + 'static>(
		state_chain_client: &StateChainClient,
		block_hash: state_chain_runtime::Hash,
	) -> TxOutIds<Inner>
	where
		state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
	{
		state_chain_client
			.storage_map::<pallet_cf_broadcast::TransactionOutIdToBroadcastId<
				state_chain_runtime::Runtime,
				<Inner::Chain as PalletInstanceAlias>::Instance,
			>, Vec<_>>(block_hash)
			.await
			.expect(STATE_CHAIN_CONNECTION)
			.into_iter()
			.map(|(tx_out_id, _)| tx_out_id)
			.collect()
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
			Self::get_transaction_out_ids(
				&*state_chain_client,
				state_chain_stream.cache().block_hash,
			)
			.await,
		);

		scope.spawn(async move {
			utilities::loop_select! {
				let _ = sender.closed() => { break Ok(()) },
				if let Some((_block_hash, _block_header)) = state_chain_stream.next() => {
					let _result = sender.send(Self::get_transaction_out_ids(&*state_chain_client, _block_hash).await);
				} else break Ok(()),
			}
		});

		Self { inner, receiver }
	}
}

#[derive(CloneNoBound)]
pub struct EgressClient<Inner: ChunkedByVault>
where
	state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
{
	inner_client: Inner::Client,
	receiver: tokio::sync::watch::Receiver<TxOutIds<Inner>>,
}
impl<Inner: ChunkedByVault> EgressClient<Inner>
where
	state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
{
	pub fn new(
		inner_client: Inner::Client,
		receiver: tokio::sync::watch::Receiver<TxOutIds<Inner>>,
	) -> Self {
		Self { inner_client, receiver }
	}
}

#[async_trait::async_trait]
impl<Inner: ChunkedByVault> ChunkedByVault for EgressItems<Inner>
where
	state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
{
	type ExtraInfo = Inner::ExtraInfo;
	type ExtraHistoricInfo = Inner::ExtraHistoricInfo;

	type Index = Inner::Index;
	type Hash = Inner::Hash;
	type Data = (Inner::Data, TxOutIds<Inner>);

	type Client = EgressClient<Inner>;

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
								if let Some(header) = chain_stream.next() => {
									// Always get the latest tx out ids.
									// NB: There is a race condition here. If we're not watching for a particular egress id (because our state chain is slow for some reason) at the time
									// it arrives on external chain, we won't witness it. This is pretty unlikely since the time between the egress id being set on the SC and the tx
									// being confirmed on the external chain is quite large. We should fix this eventually though. PRO-689
									let tx_out_ids = receiver.borrow().clone();
									break Some((header.map_data(|header| (header.data, tx_out_ids)), (chain_stream, receiver)))
								} else break None,
							)
						},
					)
					.into_box(),
					EgressClient::new(chain_client, self.receiver.clone()),
				)
			})
			.await
			.into_box()
	}
}

#[async_trait::async_trait]
impl<Inner: ChunkedByVault> ChainClient for EgressClient<Inner>
where
	state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
{
	type Index = Inner::Index;
	type Hash = Inner::Hash;
	type Data = (Inner::Data, TxOutIds<Inner>);

	async fn header_at_index(
		&self,
		index: Self::Index,
	) -> Header<Self::Index, Self::Hash, Self::Data> {
		let egress_items = self.receiver.borrow().clone();
		self.inner_client
			.header_at_index(index)
			.await
			.map_data(|header| (header.data, egress_items))
	}
}

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	pub async fn egress_items<'env, StateChainStream, StateChainClient>(
		self,
		scope: &Scope<'env, anyhow::Error>,
		state_chain_stream: StateChainStream,
		state_chain_client: Arc<StateChainClient>,
	) -> ChunkedByVaultBuilder<EgressItems<Inner>>
	where
		state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
		StateChainStream: StateChainStreamApi,
		StateChainClient: StorageApi + Send + Sync + 'static,
	{
		ChunkedByVaultBuilder::new(
			EgressItems::new(self.source, scope, state_chain_stream, state_chain_client).await,
			self.parameters,
		)
	}
}
