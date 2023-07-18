use futures::StreamExt;
use futures_core::Future;
use utilities::assert_stream_send;

use crate::witness::{
	chain_source::{aliases, Header},
	chunked_chain_source::{latest_then::LatestThen, then::Then, ChunkedChainSource},
	epoch_source::Epoch,
};

use crate::witness::common::BoxActiveAndFuture;
use cf_chains::Chain;

use super::{ChunkedByVault, Item};

pub struct ChunkedByVaultBuilder<Inner: ChunkedByVault> {
	pub source: Inner,
	pub parameters: Inner::Parameters,
}
impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	pub fn new(source: Inner, parameters: Inner::Parameters) -> Self {
		Self { source, parameters }
	}

	pub async fn run(self) {
		let stream = assert_stream_send(
			self.source
				.stream(self.parameters)
				.await
				.into_stream()
				.flat_map_unordered(None, |(_epoch, chain_stream, _chain_client)| chain_stream),
		);
		stream.for_each(|_| futures::future::ready(())).await;
	}
}

impl<T: ChunkedByVault> ChunkedByVaultBuilder<T> {
	pub fn then<Output, Fut, ThenFn>(
		self,
		then_fn: ThenFn,
	) -> ChunkedByVaultBuilder<Then<Generic<T>, ThenFn>>
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
		ChunkedByVaultBuilder {
			source: Then::new(Generic(self.source), then_fn),
			parameters: self.parameters,
		}
	}

	pub fn latest_then<Output, Fut, ThenFn>(
		self,
		then_fn: ThenFn,
	) -> ChunkedByVaultBuilder<LatestThen<Generic<T>, ThenFn>>
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
		ChunkedByVaultBuilder {
			source: LatestThen::new(Generic(self.source), then_fn),
			parameters: self.parameters,
		}
	}
}

/// Wraps a specific impl of ChunkedByVault, and impls ChunkedChainSource for it
pub struct Generic<T>(pub T);
#[async_trait::async_trait]
impl<T: ChunkedByVault> ChunkedChainSource for Generic<T> {
	type Info = pallet_cf_vaults::Vault<T::Chain>;
	type HistoricInfo = <T::Chain as Chain>::ChainBlockNumber;

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
