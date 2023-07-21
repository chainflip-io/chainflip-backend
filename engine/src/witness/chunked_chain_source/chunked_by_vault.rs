pub mod builder;
pub mod continuous;
pub mod ingress_addresses;

use cf_chains::Chain;
use futures_util::StreamExt;

use crate::witness::{
	chain_source::{aliases, BoxChainStream, ChainClient, ChainStream},
	common::{BoxActiveAndFuture, ExternalChain, ExternalChainSource, RuntimeHasChain},
	epoch_source::{Epoch, VaultSource},
};

use super::ChunkedChainSource;

#[async_trait::async_trait]
pub trait ChunkedByVault: Sized + Send + Sync {
	type Index: aliases::Index;
	type Hash: aliases::Hash;
	type Data: aliases::Data;

	type Client: ChainClient<Index = Self::Index, Hash = Self::Hash, Data = Self::Data>;

	type Chain: ExternalChain<ChainBlockNumber = Self::Index>;

	type Parameters: Send;

	async fn stream(&self, parameters: Self::Parameters) -> BoxActiveAndFuture<'_, Item<'_, Self>>;
}

pub type Item<'a, T> = (
	Epoch<
		pallet_cf_vaults::Vault<<T as ChunkedByVault>::Chain>,
		<<T as ChunkedByVault>::Chain as Chain>::ChainBlockNumber,
	>,
	BoxChainStream<
		'a,
		<T as ChunkedByVault>::Index,
		<T as ChunkedByVault>::Hash,
		<T as ChunkedByVault>::Data,
	>,
	<T as ChunkedByVault>::Client,
);

#[async_trait::async_trait]
impl<
		TChain: ExternalChain<ChainBlockNumber = T::Index>,
		T: ChunkedChainSource<
			Info = pallet_cf_vaults::Vault<TChain>,
			HistoricInfo = <TChain as Chain>::ChainBlockNumber,
			Chain = TChain,
		>,
	> ChunkedByVault for T
{
	type Index = T::Index;
	type Hash = T::Hash;
	type Data = T::Data;

	type Client = T::Client;

	type Chain = T::Chain;

	type Parameters = T::Parameters;

	async fn stream(&self, parameters: Self::Parameters) -> BoxActiveAndFuture<'_, Item<'_, Self>> {
		<Self as ChunkedChainSource>::stream(self, parameters).await
	}
}

pub struct ChunkByVault<TChainSource: ExternalChainSource>
where
	state_chain_runtime::Runtime: RuntimeHasChain<TChainSource::Chain>,
{
	chain_source: TChainSource,
}
impl<TChainSource: ExternalChainSource> ChunkByVault<TChainSource>
where
	state_chain_runtime::Runtime: RuntimeHasChain<TChainSource::Chain>,
{
	pub fn new(chain_source: TChainSource) -> Self {
		Self { chain_source }
	}
}
#[async_trait::async_trait]
impl<TChainSource: ExternalChainSource> ChunkedByVault for ChunkByVault<TChainSource>
where
	state_chain_runtime::Runtime: RuntimeHasChain<TChainSource::Chain>,
{
	type Index = TChainSource::Index;
	type Hash = TChainSource::Hash;
	type Data = TChainSource::Data;

	type Client = TChainSource::Client;

	type Chain = TChainSource::Chain;

	type Parameters = VaultSource<TChainSource::Chain>;

	async fn stream(&self, vaults: Self::Parameters) -> BoxActiveAndFuture<'_, Item<'_, Self>> {
		vaults
			.into_stream()
			.await
			.then(move |mut vault| async move {
				let (stream, client) = self.chain_source.stream_and_client().await;

				(
					vault.clone(),
					stream
						.take_until(vault.expired_signal.wait())
						.filter(move |header| {
							futures::future::ready(
								header.index >= vault.info.active_from_block &&
									vault
										.historic_signal
										.get()
										.map_or(true, |end_index| header.index < *end_index),
							)
						})
						.into_box(),
					client,
				)
			})
			.await
			.into_box()
	}
}
