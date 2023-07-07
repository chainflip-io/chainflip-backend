use cf_chains::Chain;
use futures_util::StreamExt;

use crate::witness::{
	chain_source::box_chain_stream,
	common::{BoxActiveAndFuture, ExternalChainSource, RuntimeHasChain},
	epoch_source,
};

use super::ChunkedChainSource;

#[async_trait::async_trait]
pub trait ChunkedByVault<'a>: Sized + Send {
	type ChainSource: ExternalChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::ChainSource>>;
}

pub type Item<'a, TChainSource> =
	super::Item<'a, TChainSource, VaultInfo<TChainSource>, VaultEnd<TChainSource>>;

pub type VaultInfo<TChainSource> =
	pallet_cf_vaults::Vault<<TChainSource as ExternalChainSource>::Chain>;
pub type VaultEnd<TChainSource> =
	<<TChainSource as ExternalChainSource>::Chain as Chain>::ChainBlockNumber;

#[async_trait::async_trait]
impl<
		'a,
		TChainSource: ExternalChainSource,
		T: ChunkedChainSource<
			'a,
			Info = VaultInfo<TChainSource>,
			HistoricInfo = VaultEnd<TChainSource>,
			ChainSource = TChainSource,
		>,
	> ChunkedByVault<'a> for T
{
	type ChainSource = TChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::ChainSource>> {
		<Self as ChunkedChainSource<'a>>::stream(self).await
	}
}

/// Wraps a specific impl of ChunkedByVault, and impls ChunkedChainSource for it
pub struct Generic<T>(T);
#[async_trait::async_trait]
impl<'a, TChainSource: ExternalChainSource, T: ChunkedByVault<'a, ChainSource = TChainSource>>
	ChunkedChainSource<'a> for Generic<T>
{
	type Info = VaultInfo<TChainSource>;
	type HistoricInfo = VaultEnd<TChainSource>;

	type ChainSource = TChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::ChainSource>> {
		self.0.stream().await
	}
}

pub struct ChunkByVault<'a, TChainSource: ExternalChainSource>
where
	state_chain_runtime::Runtime: RuntimeHasChain<TChainSource::Chain>,
{
	chain_source: &'a TChainSource,
	vaults: BoxActiveAndFuture<'static, epoch_source::Vault<TChainSource::Chain>>,
}
#[async_trait::async_trait]
impl<'a, TChainSource: ExternalChainSource> ChunkedByVault<'a> for ChunkByVault<'a, TChainSource>
where
	state_chain_runtime::Runtime: RuntimeHasChain<TChainSource::Chain>,
{
	type ChainSource = TChainSource;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self::ChainSource>> {
		let chain_source = self.chain_source;
		self.vaults
			.then(move |mut vault| async move {
				let (stream, client) = chain_source.stream_and_client().await;

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
