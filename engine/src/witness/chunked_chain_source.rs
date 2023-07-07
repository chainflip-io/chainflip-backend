pub mod chunked_by_time;
pub mod chunked_by_vault;
pub mod map;

use futures_util::StreamExt;

use super::{
	chain_source::{BoxChainStream, ChainSource},
	common::BoxActiveAndFuture,
	epoch_source::Epoch,
};

use utilities::assert_stream_send;

#[async_trait::async_trait]
pub trait ChunkedChainSource<'a>: Sized + Send {
	type Info: Clone + Send + Sync + 'static;
	type HistoricInfo: Clone + Send + Sync + 'static;
	type ChainSource: ChainSource;

	async fn stream(
		self,
	) -> BoxActiveAndFuture<'a, Item<'a, Self::ChainSource, Self::Info, Self::HistoricInfo>>;

	async fn run(self) {
		let stream = assert_stream_send(
			self.stream()
				.await
				.into_stream()
				.flat_map_unordered(None, |(_epoch, chain_stream, _)| chain_stream),
		);
		stream.for_each(|_| futures::future::ready(())).await;
	}
}

pub type Item<'a, TChainSource, Info, HistoricInfo> = (
	Epoch<Info, HistoricInfo>,
	BoxChainStream<
		'a,
		<TChainSource as ChainSource>::Index,
		<TChainSource as ChainSource>::Hash,
		<TChainSource as ChainSource>::Data,
	>,
	<TChainSource as ChainSource>::Client,
);
