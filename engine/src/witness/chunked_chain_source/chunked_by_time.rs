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
	type InnerChainSource: ChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::InnerChainSource>>;
}

pub type Item<'a, InnerChainSource> = super::Item<'a, InnerChainSource, (), ()>;

#[async_trait::async_trait]
impl<
		'a,
		TInnerChainSource: ChainSource,
		T: ChunkedChainSource<'a, Info = (), HistoricInfo = (), InnerChainSource = TInnerChainSource>,
	> ChunkedByTime<'a> for T
{
	type InnerChainSource = TInnerChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::InnerChainSource>> {
		<Self as ChunkedByTime<'a>>::stream(self).await
	}
}

/// Wraps a specific impl of ChunkedByTime, and impls ChunkedChainSource for it
pub struct Generic<T>(T);
#[async_trait::async_trait]
impl<
		'a,
		TInnerChainSource: ChainSource,
		T: ChunkedByTime<'a, InnerChainSource = TInnerChainSource>,
	> ChunkedChainSource<'a> for Generic<T>
{
	type Info = ();
	type HistoricInfo = ();

	type InnerChainSource = TInnerChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::InnerChainSource>> {
		self.0.stream().await
	}
}

pub struct ChunkByTime<'a, InnerChainSource> {
	inner_chain_source: &'a InnerChainSource,
	epochs: BoxActiveAndFuture<'static, Epoch<(), ()>>,
}

#[async_trait::async_trait]
impl<'a, InnerChainSource: ChainSource> ChunkedByTime<'a> for ChunkByTime<'a, InnerChainSource> {
	type InnerChainSource = InnerChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::InnerChainSource>> {
		let inner_chain_source = self.inner_chain_source;
		self.epochs
			.then(move |epoch| async move {
				let (stream, client) = inner_chain_source.stream_and_client().await;
				let historic_signal = epoch.historic_signal.clone();
				(epoch, box_chain_stream(stream.take_until(historic_signal.wait())), client)
			})
			.await
			.into_box()
	}
}
