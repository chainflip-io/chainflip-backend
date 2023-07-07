pub mod chain_tracking;

use futures_util::StreamExt;

use crate::witness::{
	chain_source::{aliases, box_chain_stream, BoxChainStream, ChainClient},
	common::{BoxActiveAndFuture, ExternalChain, ExternalChainSource},
	epoch_source::Epoch,
};

use super::ChunkedChainSource;

#[async_trait::async_trait]
pub trait ChunkedByTime<'a>: Sized + Send {
	type Index: aliases::Index;
	type Hash: aliases::Hash;
	type Data: aliases::Data;

	type Client: ChainClient<Index = Self::Index, Hash = Self::Hash, Data = Self::Data>;

	type Chain: ExternalChain<ChainBlockNumber = Self::Index>;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self>>;
}

pub type Item<'a, T> = (
	Epoch<(), ()>,
	BoxChainStream<
		'a,
		<T as ChunkedByTime<'a>>::Index,
		<T as ChunkedByTime<'a>>::Hash,
		<T as ChunkedByTime<'a>>::Data,
	>,
	<T as ChunkedByTime<'a>>::Client,
);

#[async_trait::async_trait]
impl<'a, T: ChunkedChainSource<'a, Info = (), HistoricInfo = ()>> ChunkedByTime<'a> for T {
	type Index = T::Index;
	type Hash = T::Hash;
	type Data = T::Data;

	type Client = T::Client;

	type Chain = T::Chain;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self>> {
		<Self as ChunkedByTime<'a>>::stream(self).await
	}
}

/// Wraps a specific impl of ChunkedByTime, and impls ChunkedChainSource for it
pub struct Generic<T>(T);
#[async_trait::async_trait]
impl<'a, T: ChunkedByTime<'a>> ChunkedChainSource<'a> for Generic<T> {
	type Info = ();
	type HistoricInfo = ();

	type Index = T::Index;
	type Hash = T::Hash;
	type Data = T::Data;

	type Client = T::Client;

	type Chain = T::Chain;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self>> {
		self.0.stream().await
	}
}

pub struct ChunkByTime<'a, TChainSource> {
	chain_source: &'a TChainSource,
	epochs: BoxActiveAndFuture<'static, Epoch<(), ()>>,
}
impl<'a, TChainSource> ChunkByTime<'a, TChainSource> {
	pub fn new(
		chain_source: &'a TChainSource,
		epochs: BoxActiveAndFuture<'static, Epoch<(), ()>>,
	) -> Self {
		Self { chain_source, epochs }
	}
}
#[async_trait::async_trait]
impl<'a, TChainSource: ExternalChainSource> ChunkedByTime<'a> for ChunkByTime<'a, TChainSource> {
	type Index = TChainSource::Index;
	type Hash = TChainSource::Hash;
	type Data = TChainSource::Data;

	type Client = TChainSource::Client;

	type Chain = TChainSource::Chain;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self>> {
		let chain_source = self.chain_source;
		self.epochs
			.then(move |epoch| async move {
				let (stream, client) = chain_source.stream_and_client().await;
				let historic_signal = epoch.historic_signal.clone();
				(epoch, box_chain_stream(stream.take_until(historic_signal.wait())), client)
			})
			.await
			.into_box()
	}
}
