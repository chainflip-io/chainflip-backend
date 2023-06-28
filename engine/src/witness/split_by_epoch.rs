use futures_util::StreamExt;

use super::{
	chain_source::{box_chain_stream, BoxChainStream, ChainSource},
	common::BoxActiveAndFuture,
	epoch_source::Epoch,
};

use utilities::assert_stream_send;

#[async_trait::async_trait]
pub trait ChainSplitByEpoch<'a>: Sized + Send {
	type UnderlyingChainSource: ChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::UnderlyingChainSource>>;

	async fn run(self) {
		let stream = assert_stream_send(
			self.stream()
				.await
				.into_stream()
				.flat_map_unordered(None, |(_, chain_stream)| chain_stream),
		);
		stream.for_each(|_| futures::future::ready(())).await;
	}
}

pub type Item<'a, UnderlyingChainSource> = (
	Epoch<(), ()>,
	BoxChainStream<
		'a,
		<UnderlyingChainSource as ChainSource>::Index,
		<UnderlyingChainSource as ChainSource>::Hash,
		<UnderlyingChainSource as ChainSource>::Data,
	>,
);

pub struct SplitByEpoch<'a, UnderlyingChainSource> {
	underlying_chain_source: &'a UnderlyingChainSource,
	epochs: BoxActiveAndFuture<'static, Epoch<(), ()>>,
}

#[async_trait::async_trait]
impl<'a, UnderlyingChainSource: ChainSource> ChainSplitByEpoch<'a>
	for SplitByEpoch<'a, UnderlyingChainSource>
{
	type UnderlyingChainSource = UnderlyingChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::UnderlyingChainSource>> {
		let underlying_chain_source = self.underlying_chain_source;
		self.epochs
			.then(move |epoch| async move {
				let historic_signal = epoch.historic_signal.clone();
				(
					epoch,
					box_chain_stream(
						underlying_chain_source.stream().await.take_until(historic_signal.wait()),
					),
				)
			})
			.await
			.into_box()
	}
}
