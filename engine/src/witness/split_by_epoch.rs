use cf_primitives::EpochIndex;
use futures_util::{stream, StreamExt};

use super::{
	chain_source::{box_chain_stream, BoxChainStream, ChainSource},
	common::{BoxCurrentAndFuture, CurrentAndFuture},
	epoch_source::Epoch,
};

use utilities::assert_stream_send;

#[async_trait::async_trait]
pub trait ChainSplitByEpoch<'a>: Sized {
	type UnderlyingChainSource: ChainSource;

	async fn stream(self) -> BoxCurrentAndFuture<'a, Item<'a, Self::UnderlyingChainSource>>;

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

type Item<'a, UnderlyingChainSource> = (
	EpochIndex,
	BoxChainStream<
		'a,
		<UnderlyingChainSource as ChainSource>::Index,
		<UnderlyingChainSource as ChainSource>::Hash,
		<UnderlyingChainSource as ChainSource>::Data,
	>,
);

pub struct SplitByEpoch<'a, UnderlyingChainSource> {
	underlying_chain_source: &'a UnderlyingChainSource,
	epochs: BoxCurrentAndFuture<'static, Epoch<(), ()>>,
}
impl<'a, UnderlyingChainSource: ChainSource> SplitByEpoch<'a, UnderlyingChainSource> {
	async fn into_item(
		underlying_chain_source: &'a UnderlyingChainSource,
		epoch: Epoch<(), ()>,
	) -> Item<'a, UnderlyingChainSource> {
		let stream = underlying_chain_source.stream().await;

		(epoch.epoch, box_chain_stream(stream.take_until(epoch.historic_signal.wait())))
	}
}
#[async_trait::async_trait]
impl<'a, UnderlyingChainSource: ChainSource> ChainSplitByEpoch<'a>
	for SplitByEpoch<'a, UnderlyingChainSource>
{
	type UnderlyingChainSource = UnderlyingChainSource;

	async fn stream(self) -> BoxCurrentAndFuture<'a, Item<'a, Self::UnderlyingChainSource>> {
		CurrentAndFuture {
			current: Box::new(
				stream::iter(self.epochs.current)
					.then(|epoch| Self::into_item(self.underlying_chain_source, epoch))
					.collect::<Vec<_>>()
					.await
					.into_iter(),
			),
			future: Box::pin(
				self.epochs
					.future
					.then(|epoch| Self::into_item(self.underlying_chain_source, epoch)),
			),
		}
	}
}
