use futures_util::StreamExt;
use utilities::{assert_stream_send, task_scope::Scope};

use crate::witness::common::{
	chain_source::{aliases, Header},
	chunked_chain_source::{latest_then::LatestThen, then::Then, ChunkedChainSource},
	epoch_source::Epoch,
};

use super::{ChunkedByTime, Item};
use futures::Future;

use crate::witness::common::BoxActiveAndFuture;

pub struct ChunkedByTimeBuilder<Inner: ChunkedByTime> {
	pub source: Inner,
	pub parameters: Inner::Parameters,
}

impl<Inner: ChunkedByTime + Clone> Clone for ChunkedByTimeBuilder<Inner>
where
	Inner::Parameters: Clone,
{
	fn clone(&self) -> Self {
		Self { source: self.source.clone(), parameters: self.parameters.clone() }
	}
}

impl<Inner: ChunkedByTime> ChunkedByTimeBuilder<Inner> {
	pub fn new(source: Inner, parameters: Inner::Parameters) -> Self {
		Self { source, parameters }
	}

	pub fn spawn<'env>(self, scope: &Scope<'env, anyhow::Error>)
	where
		Inner: 'env,
	{
		scope.spawn(async move {
			let stream = assert_stream_send(
				self.source
					.stream(self.parameters)
					.await
					.into_stream()
					.flat_map_unordered(None, |(_epoch, chain_stream, _chain_client)| chain_stream),
			);
			stream.for_each(|_| futures::future::ready(())).await;
			Ok(())
		});
	}
}

impl<T: ChunkedByTime> ChunkedByTimeBuilder<T> {
	pub fn then<Output, Fut, ThenFn>(
		self,
		then_fn: ThenFn,
	) -> ChunkedByTimeBuilder<Then<Generic<T>, ThenFn>>
	where
		Output: aliases::Data,
		Fut: Future<Output = Output> + Send,
		ThenFn: Fn(
				Epoch<
					<Generic<T> as ChunkedChainSource>::Info,
					<Generic<T> as ChunkedChainSource>::HistoricInfo,
				>,
				Header<T::Index, T::Hash, T::Data>,
			) -> Fut
			+ Send
			+ Sync
			+ Clone,
	{
		ChunkedByTimeBuilder {
			source: Then::new(Generic(self.source), then_fn),
			parameters: self.parameters,
		}
	}

	pub fn latest_then<Output, Fut, ThenFn>(
		self,
		then_fn: ThenFn,
	) -> ChunkedByTimeBuilder<LatestThen<Generic<T>, ThenFn>>
	where
		Output: aliases::Data,
		Fut: Future<Output = Output> + Send,
		ThenFn: Fn(
				Epoch<
					<Generic<T> as ChunkedChainSource>::Info,
					<Generic<T> as ChunkedChainSource>::HistoricInfo,
				>,
				Header<T::Index, T::Hash, T::Data>,
			) -> Fut
			+ Send
			+ Sync
			+ Clone,
	{
		ChunkedByTimeBuilder {
			source: LatestThen::new(Generic(self.source), then_fn),
			parameters: self.parameters,
		}
	}
}

/// Wraps a specific impl of ChunkedByTime, and impls ChunkedChainSource for it
pub struct Generic<T>(pub T);
#[async_trait::async_trait]
impl<T: ChunkedByTime> ChunkedChainSource for Generic<T> {
	type Info = ();
	type HistoricInfo = ();

	type Index = T::Index;
	type Hash = T::Hash;
	type Data = T::Data;

	type Client = T::Client;

	type Chain = T::Chain;

	type Parameters = T::Parameters;

	async fn stream(&self, parameters: Self::Parameters) -> BoxActiveAndFuture<'_, Item<'_, Self>> {
		self.0.stream(parameters).await
	}
}
