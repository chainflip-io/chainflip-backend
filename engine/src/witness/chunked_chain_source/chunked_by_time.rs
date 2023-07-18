pub mod builder;
pub mod chain_tracking;

use futures_util::StreamExt;

use crate::witness::{
	chain_source::{aliases, BoxChainStream, ChainClient, ChainStream},
	common::{BoxActiveAndFuture, ExternalChain, ExternalChainSource},
	epoch_source::Epoch,
};

use super::ChunkedChainSource;

#[async_trait::async_trait]
pub trait ChunkedByTime: Sized + Send + Sync {
	type Index: aliases::Index;
	type Hash: aliases::Hash;
	type Data: aliases::Data;

	type Client: ChainClient<Index = Self::Index, Hash = Self::Hash, Data = Self::Data>;

	type Chain: ExternalChain<ChainBlockNumber = Self::Index>;

	type Parameters: Send;

	async fn stream(&self, parameters: Self::Parameters) -> BoxActiveAndFuture<'_, Item<'_, Self>>;
}

pub type Item<'a, T> = (
	Epoch<(), ()>,
	BoxChainStream<
		'a,
		<T as ChunkedByTime>::Index,
		<T as ChunkedByTime>::Hash,
		<T as ChunkedByTime>::Data,
	>,
	<T as ChunkedByTime>::Client,
);

#[async_trait::async_trait]
impl<T: ChunkedChainSource<Info = (), HistoricInfo = ()>> ChunkedByTime for T {
	type Index = T::Index;
	type Hash = T::Hash;
	type Data = T::Data;

	type Client = T::Client;

	type Chain = T::Chain;

	type Parameters = T::Parameters;

	async fn stream(&self, parameters: Self::Parameters) -> BoxActiveAndFuture<'_, Item<'_, Self>> {
		<Self as ChunkedByTime>::stream(self, parameters).await
	}
}

pub struct ChunkByTime<TChainSource> {
	chain_source: TChainSource,
}
impl<TChainSource> ChunkByTime<TChainSource> {
	pub fn new(chain_source: TChainSource) -> Self {
		Self { chain_source }
	}
}
#[async_trait::async_trait]
impl<TChainSource: ExternalChainSource> ChunkedByTime for ChunkByTime<TChainSource> {
	type Index = TChainSource::Index;
	type Hash = TChainSource::Hash;
	type Data = TChainSource::Data;

	type Client = TChainSource::Client;

	type Chain = TChainSource::Chain;

	type Parameters = BoxActiveAndFuture<'static, Epoch<(), ()>>;

	async fn stream(&self, epochs: Self::Parameters) -> BoxActiveAndFuture<'_, Item<'_, Self>> {
		epochs
			.then(move |epoch| async move {
				let (stream, client) = self.chain_source.stream_and_client().await;
				let historic_signal = epoch.historic_signal.clone();
				(epoch, stream.take_until(historic_signal.wait()).into_box(), client)
			})
			.await
			.into_box()
	}
}
