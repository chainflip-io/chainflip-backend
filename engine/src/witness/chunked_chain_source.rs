pub mod chunked_by_time;
pub mod chunked_by_vault;

use futures_util::StreamExt;

use super::{
	chain_source::{BoxChainStream, ChainSource},
	common::BoxActiveAndFuture,
	epoch_source::Epoch,
};

use utilities::assert_stream_send;

#[async_trait::async_trait]
pub trait ChunkedChainSource<'a>: Sized + Send {
	type Info: Clone;
	type HistoricInfo: Clone;
	type UnderlyingChainSource: ChainSource;

	async fn stream(
		self,
	) -> BoxActiveAndFuture<'a, Item<'a, Self::UnderlyingChainSource, Self::Info, Self::HistoricInfo>>;

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

pub type Item<'a, UnderlyingChainSource, Info, HistoricInfo> = (
	Epoch<Info, HistoricInfo>,
	BoxChainStream<
		'a,
		<UnderlyingChainSource as ChainSource>::Index,
		<UnderlyingChainSource as ChainSource>::Hash,
		<UnderlyingChainSource as ChainSource>::Data,
	>,
	<UnderlyingChainSource as ChainSource>::Client,
);
