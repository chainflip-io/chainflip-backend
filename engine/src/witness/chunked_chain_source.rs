pub mod chunked_by_time;
pub mod chunked_by_vault;
pub mod extension;
pub mod map;

use futures_util::StreamExt;

use super::{
	chain_source::{aliases, BoxChainStream, ChainClient},
	common::{BoxActiveAndFuture, ExternalChain},
	epoch_source::Epoch,
};

use utilities::assert_stream_send;

#[async_trait::async_trait]
pub trait ChunkedChainSource: Sized + Send + Sync {
	type Info: Clone + Send + Sync + 'static;
	type HistoricInfo: Clone + Send + Sync + 'static;

	type Index: aliases::Index;
	type Hash: aliases::Hash;
	type Data: aliases::Data;

	type Client: ChainClient<Index = Self::Index, Hash = Self::Hash, Data = Self::Data>;

	type Chain: ExternalChain<ChainBlockNumber = Self::Index>;

	type Parameters: Send;

	async fn stream(
		&self,
		parameters: Self::Parameters,
	) -> BoxActiveAndFuture<'_, Item<'_, Self, Self::Info, Self::HistoricInfo>>;
}

pub type Item<'a, T, Info, HistoricInfo> = (
	Epoch<Info, HistoricInfo>,
	BoxChainStream<
		'a,
		<T as ChunkedChainSource>::Index,
		<T as ChunkedChainSource>::Hash,
		<T as ChunkedChainSource>::Data,
	>,
	<T as ChunkedChainSource>::Client,
);

pub struct Builder<T: ChunkedChainSource> {
	source: T,
	parameters: T::Parameters,
}
impl<T: ChunkedChainSource> Builder<T> {
	pub fn new(source: T, parameters: T::Parameters) -> Self {
		Self { source, parameters }
	}

	pub async fn run(self) {
		let stream = assert_stream_send(
			self.source
				.stream(self.parameters)
				.await
				.into_stream()
				.flat_map_unordered(None, |(_epoch, chain_stream, _chain_client)| chain_stream),
		);
		stream.for_each(|_| futures::future::ready(())).await;
	}
}
