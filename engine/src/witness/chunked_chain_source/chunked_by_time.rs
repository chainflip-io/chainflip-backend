pub mod chain_tracking;

use futures_util::StreamExt;

use crate::witness::{
	chain_source::{box_chain_stream, ChainSource},
	common::BoxActiveAndFuture,
	epoch_source::Epoch,
};

use super::ChunkedChainSource;

#[async_trait::async_trait]
pub trait ChunkedByTime<'a>: Sized + Send {
	type ChainSource: ChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::ChainSource>>;
}

pub type Item<'a, TChainSource> = super::Item<'a, TChainSource, (), ()>;

#[async_trait::async_trait]
impl<
		'a,
		TChainSource: ChainSource,
		T: ChunkedChainSource<'a, Info = (), HistoricInfo = (), ChainSource = TChainSource>,
	> ChunkedByTime<'a> for T
{
	type ChainSource = TChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::ChainSource>> {
		<Self as ChunkedByTime<'a>>::stream(self).await
	}
}

/// Wraps a specific impl of ChunkedByTime, and impls ChunkedChainSource for it
pub struct Generic<T>(T);
#[async_trait::async_trait]
impl<'a, TChainSource: ChainSource, T: ChunkedByTime<'a, ChainSource = TChainSource>>
	ChunkedChainSource<'a> for Generic<T>
{
	type Info = ();
	type HistoricInfo = ();

	type ChainSource = TChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::ChainSource>> {
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
impl<'a, TChainSource: ChainSource> ChunkedByTime<'a> for ChunkByTime<'a, TChainSource> {
	type ChainSource = TChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::ChainSource>> {
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
