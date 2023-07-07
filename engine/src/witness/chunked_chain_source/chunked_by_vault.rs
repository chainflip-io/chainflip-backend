use cf_chains::Chain;
use futures_util::StreamExt;

use crate::witness::{
	chain_source::{aliases, box_chain_stream, BoxChainStream, ChainClient},
	common::{BoxActiveAndFuture, ExternalChain, ExternalChainSource, RuntimeHasChain},
	epoch_source::{self, Epoch},
};

use super::ChunkedChainSource;

#[async_trait::async_trait]
pub trait ChunkedByVault<'a>: Sized + Send {
	type Index: aliases::Index;
	type Hash: aliases::Hash;
	type Data: aliases::Data;

	type Client: ChainClient<Index = Self::Index, Hash = Self::Hash, Data = Self::Data>;

	type Chain: ExternalChain<ChainBlockNumber = Self::Index>;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self>>;
}

pub type Item<'a, T> = (
	Epoch<
		pallet_cf_vaults::Vault<<T as ChunkedByVault<'a>>::Chain>,
		<<T as ChunkedByVault<'a>>::Chain as Chain>::ChainBlockNumber,
	>,
	BoxChainStream<
		'a,
		<T as ChunkedByVault<'a>>::Index,
		<T as ChunkedByVault<'a>>::Hash,
		<T as ChunkedByVault<'a>>::Data,
	>,
	<T as ChunkedByVault<'a>>::Client,
);

#[async_trait::async_trait]
impl<
		'a,
		TChain: ExternalChain<ChainBlockNumber = T::Index>,
		T: ChunkedChainSource<
			'a,
			Info = pallet_cf_vaults::Vault<TChain>,
			HistoricInfo = <TChain as Chain>::ChainBlockNumber,
			Chain = TChain,
		>,
	> ChunkedByVault<'a> for T
{
	type Index = T::Index;
	type Hash = T::Hash;
	type Data = T::Data;

	type Client = T::Client;

	type Chain = T::Chain;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self>> {
		<Self as ChunkedChainSource<'a>>::stream(self).await
	}
}

/// Wraps a specific impl of ChunkedByVault, and impls ChunkedChainSource for it
pub struct Generic<T>(T);
#[async_trait::async_trait]
impl<'a, T: ChunkedByVault<'a>> ChunkedChainSource<'a> for Generic<T> {
	type Info = pallet_cf_vaults::Vault<T::Chain>;
	type HistoricInfo = <T::Chain as Chain>::ChainBlockNumber;

	type Index = T::Index;
	type Hash = T::Hash;
	type Data = T::Data;

	type Client = T::Client;

	type Chain = T::Chain;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self>> {
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
impl<'a, TChainSource: ExternalChainSource> ChunkByVault<'a, TChainSource>
where
	state_chain_runtime::Runtime: RuntimeHasChain<TChainSource::Chain>,
{
	pub fn new(
		chain_source: &'a TChainSource,
		vaults: BoxActiveAndFuture<'static, epoch_source::Vault<TChainSource::Chain>>,
	) -> Self {
		Self { chain_source, vaults }
	}
}
#[async_trait::async_trait]
impl<'a, TChainSource: ExternalChainSource> ChunkedByVault<'a> for ChunkByVault<'a, TChainSource>
where
	state_chain_runtime::Runtime: RuntimeHasChain<TChainSource::Chain>,
{
	type Index = TChainSource::Index;
	type Hash = TChainSource::Hash;
	type Data = TChainSource::Data;

	type Client = TChainSource::Client;

	type Chain = TChainSource::Chain;

	async fn stream(self) -> BoxActiveAndFuture<'a, Item<'a, Self>> {
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
