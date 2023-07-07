use cf_chains::Chain;
use futures_util::StreamExt;

use crate::witness::{
	chain_source::box_chain_stream,
	common::{BoxActiveAndFuture, ExternalChainSource, RuntimeHasChain},
	epoch_source,
};

use super::ChunkedChainSource;

#[async_trait::async_trait]
pub trait ChunkedByVault<'a>: Sized + Send
where
	state_chain_runtime::Runtime:
		RuntimeHasChain<<Self::UnderlyingChainSource as ExternalChainSource>::Chain>,
{
	type UnderlyingChainSource: ExternalChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::UnderlyingChainSource>>;
}

pub type Item<'a, UnderlyingChainSource> = super::Item<
	'a,
	UnderlyingChainSource,
	pallet_cf_vaults::Vault<<UnderlyingChainSource as ExternalChainSource>::Chain>,
	<<UnderlyingChainSource as ExternalChainSource>::Chain as Chain>::ChainBlockNumber,
>;

#[async_trait::async_trait]
impl<
	'a,
	TUnderlyingChainSource: ExternalChainSource,
	T: ChunkedChainSource<
		'a,
		Info = pallet_cf_vaults::Vault<<TUnderlyingChainSource as ExternalChainSource>::Chain>,
		HistoricInfo = <<TUnderlyingChainSource as ExternalChainSource>::Chain as Chain>::ChainBlockNumber,
		UnderlyingChainSource = TUnderlyingChainSource
	>
> ChunkedByVault<'a> for T where
state_chain_runtime::Runtime:
	RuntimeHasChain<<TUnderlyingChainSource as ExternalChainSource>::Chain>, {

	type UnderlyingChainSource = TUnderlyingChainSource;

	async fn stream(
		self,
	) -> BoxActiveAndFuture<'a, Item<'a, Self::UnderlyingChainSource>>
	{
		<Self as ChunkedChainSource<'a>>::stream(self).await
	}
}

/// Wraps a specific impl of ChunkedByVault, and impls ChunkedChainSource for it
pub struct Generic<T>(T);
#[async_trait::async_trait]
impl<
		'a,
		TUnderlyingChainSource: ExternalChainSource,
		T: ChunkedByVault<'a, UnderlyingChainSource = TUnderlyingChainSource>,
	> ChunkedChainSource<'a> for Generic<T>
where
	state_chain_runtime::Runtime:
		RuntimeHasChain<<T::UnderlyingChainSource as ExternalChainSource>::Chain>,
{
	type Info = pallet_cf_vaults::Vault<<TUnderlyingChainSource as ExternalChainSource>::Chain>;
	type HistoricInfo =
		<<TUnderlyingChainSource as ExternalChainSource>::Chain as Chain>::ChainBlockNumber;

	type UnderlyingChainSource = TUnderlyingChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::UnderlyingChainSource>> {
		self.0.stream().await
	}
}

pub struct ChunkByVault<'a, UnderlyingChainSource: ExternalChainSource>
where
	state_chain_runtime::Runtime: RuntimeHasChain<UnderlyingChainSource::Chain>,
{
	underlying_chain_source: &'a UnderlyingChainSource,
	vaults: BoxActiveAndFuture<'static, epoch_source::Vault<UnderlyingChainSource::Chain>>,
}
#[async_trait::async_trait]
impl<'a, UnderlyingChainSource: ExternalChainSource> ChunkedByVault<'a>
	for ChunkByVault<'a, UnderlyingChainSource>
where
	state_chain_runtime::Runtime: RuntimeHasChain<UnderlyingChainSource::Chain>,
{
	type UnderlyingChainSource = UnderlyingChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::UnderlyingChainSource>> {
		let underlying_chain_source = self.underlying_chain_source;
		self.vaults
			.then(move |mut vault| async move {
				let (stream, client) = underlying_chain_source.stream_and_client().await;

				(
					vault.clone(),
					box_chain_stream(stream.take_until(vault.expired_signal.wait()).filter(
						move |header| {
							futures::future::ready(
								header.index >= vault.info.active_from_block &&
									vault
										.historic_signal
										.get()
										.map_or(true, |end_index| header.index < *end_index),
							)
						},
					)),
					client,
				)
			})
			.await
			.into_box()
	}
}
