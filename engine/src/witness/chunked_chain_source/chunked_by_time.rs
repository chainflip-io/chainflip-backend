use futures_util::StreamExt;

use crate::witness::{
	chain_source::{box_chain_stream, ChainSource},
	common::BoxActiveAndFuture,
	epoch_source::Epoch,
};

use super::ChunkedChainSource;

#[async_trait::async_trait]
pub trait ChunkedByTime<'a>: Sized + Send {
	type UnderlyingChainSource: ChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::UnderlyingChainSource>>;
}

pub type Item<'a, UnderlyingChainSource> = super::Item<'a, UnderlyingChainSource, (), ()>;

#[async_trait::async_trait]
impl<
		'a,
		TUnderlyingChainSource: ChainSource,
		T: ChunkedChainSource<
			'a,
			Info = (),
			HistoricInfo = (),
			UnderlyingChainSource = TUnderlyingChainSource,
		>,
	> ChunkedByTime<'a> for T
{
	type UnderlyingChainSource = TUnderlyingChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::UnderlyingChainSource>> {
		<Self as ChunkedByTime<'a>>::stream(self).await
	}
}

/// Wraps a specific impl of ChunkedByTime, and impls ChunkedChainSource for it
pub struct Generic<T>(T);
#[async_trait::async_trait]
impl<
		'a,
		TUnderlyingChainSource: ChainSource,
		T: ChunkedByTime<'a, UnderlyingChainSource = TUnderlyingChainSource>,
	> ChunkedChainSource<'a> for Generic<T>
{
	type Info = ();
	type HistoricInfo = ();

	type UnderlyingChainSource = TUnderlyingChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::UnderlyingChainSource>> {
		self.0.stream().await
	}
}

pub struct ChunkByTime<'a, UnderlyingChainSource> {
	underlying_chain_source: &'a UnderlyingChainSource,
	epochs: BoxActiveAndFuture<'static, Epoch<(), ()>>,
}

#[async_trait::async_trait]
impl<'a, UnderlyingChainSource: ChainSource> ChunkedByTime<'a>
	for ChunkByTime<'a, UnderlyingChainSource>
{
	type UnderlyingChainSource = UnderlyingChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::UnderlyingChainSource>> {
		let underlying_chain_source = self.underlying_chain_source;
		self.epochs
			.then(move |epoch| async move {
				let (stream, client) = underlying_chain_source.stream_and_client().await;
				let historic_signal = epoch.historic_signal.clone();
				(epoch, box_chain_stream(stream.take_until(historic_signal.wait())), client)
			})
			.await
			.into_box()
	}
}
