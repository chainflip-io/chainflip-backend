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
		RuntimeHasChain<<Self::InnerChainSource as ExternalChainSource>::Chain>,
{
	type InnerChainSource: ExternalChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::InnerChainSource>>;
}

pub type Item<'a, InnerChainSource> =
	super::Item<'a, InnerChainSource, VaultInfo<InnerChainSource>, VaultEnd<InnerChainSource>>;

pub type VaultInfo<InnerChainSource> =
	pallet_cf_vaults::Vault<<InnerChainSource as ExternalChainSource>::Chain>;
pub type VaultEnd<InnerChainSource> =
	<<InnerChainSource as ExternalChainSource>::Chain as Chain>::ChainBlockNumber;

#[async_trait::async_trait]
impl<
		'a,
		TInnerChainSource: ExternalChainSource,
		T: ChunkedChainSource<
			'a,
			Info = VaultInfo<TInnerChainSource>,
			HistoricInfo = VaultEnd<TInnerChainSource>,
			InnerChainSource = TInnerChainSource,
		>,
	> ChunkedByVault<'a> for T
where
	state_chain_runtime::Runtime:
		RuntimeHasChain<<TInnerChainSource as ExternalChainSource>::Chain>,
{
	type InnerChainSource = TInnerChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::InnerChainSource>> {
		<Self as ChunkedChainSource<'a>>::stream(self).await
	}
}

/// Wraps a specific impl of ChunkedByVault, and impls ChunkedChainSource for it
pub struct Generic<T>(T);
#[async_trait::async_trait]
impl<
		'a,
		TInnerChainSource: ExternalChainSource,
		T: ChunkedByVault<'a, InnerChainSource = TInnerChainSource>,
	> ChunkedChainSource<'a> for Generic<T>
where
	state_chain_runtime::Runtime:
		RuntimeHasChain<<T::InnerChainSource as ExternalChainSource>::Chain>,
{
	type Info = VaultInfo<TInnerChainSource>;
	type HistoricInfo = VaultEnd<TInnerChainSource>;

	type InnerChainSource = TInnerChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::InnerChainSource>> {
		self.0.stream().await
	}
}

pub struct ChunkByVault<'a, InnerChainSource: ExternalChainSource>
where
	state_chain_runtime::Runtime: RuntimeHasChain<InnerChainSource::Chain>,
{
	inner_chain_source: &'a InnerChainSource,
	vaults: BoxActiveAndFuture<'static, epoch_source::Vault<InnerChainSource::Chain>>,
}
#[async_trait::async_trait]
impl<'a, InnerChainSource: ExternalChainSource> ChunkedByVault<'a>
	for ChunkByVault<'a, InnerChainSource>
where
	state_chain_runtime::Runtime: RuntimeHasChain<InnerChainSource::Chain>,
{
	type InnerChainSource = InnerChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::InnerChainSource>> {
		let inner_chain_source = self.inner_chain_source;
		self.vaults
			.then(move |mut vault| async move {
				let (stream, client) = inner_chain_source.stream_and_client().await;

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
