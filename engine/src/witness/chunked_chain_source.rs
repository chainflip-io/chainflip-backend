pub mod chunked_by_time;
pub mod chunked_by_vault;
pub mod map;

use futures_util::StreamExt;

use super::{
	chain_source::{aliases, BoxChainStream, ChainClient},
	common::{BoxActiveAndFuture, ExternalChain},
	epoch_source::Epoch,
};

use utilities::assert_stream_send;

#[async_trait::async_trait]
pub trait ChunkedChainSource<'a>: Sized + Send {
	type Info: Clone + Send + Sync + 'static;
	type HistoricInfo: Clone + Send + Sync + 'static;

	type Index: aliases::Index;
	type Hash: aliases::Hash;
	type Data: aliases::Data;

	type Client: ChainClient<Index = Self::Index, Hash = Self::Hash, Data = Self::Data>;

	type Chain: ExternalChain<ChainBlockNumber = Self::Index>;

	async fn stream(self)
		-> BoxActiveAndFuture<'a, Item<'a, Self, Self::Info, Self::HistoricInfo>>;

	async fn run(self) {
		let stream = assert_stream_send(
			self.stream()
				.await
				.into_stream()
				.flat_map_unordered(None, |(_epoch, chain_stream, _chain_client)| chain_stream),
		);
		stream.for_each(|_| futures::future::ready(())).await;
	}
}

pub type Item<'a, T, Info, HistoricInfo> = (
	Epoch<Info, HistoricInfo>,
	BoxChainStream<
		'a,
		<T as ChunkedChainSource<'a>>::Index,
		<T as ChunkedChainSource<'a>>::Hash,
		<T as ChunkedChainSource<'a>>::Data,
	>,
	<T as ChunkedChainSource<'a>>::Client,
);
