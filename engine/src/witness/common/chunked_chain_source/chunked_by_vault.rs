pub mod builder;
pub mod continuous;
pub mod deposit_addresses;
pub mod egress_items;
pub mod monitored_items;

use cf_chains::{Chain, ChainCrypto};
use futures_util::StreamExt;

use crate::witness::common::{
	chain_source::{aliases, BoxChainStream, ChainClient, ChainStream},
	epoch_source::{Vault, VaultSource},
	BoxActiveAndFuture, ExternalChain, ExternalChainSource, RuntimeHasChain,
};

use super::ChunkedChainSource;

#[async_trait::async_trait]
pub trait ChunkedByVault: Sized + Send + Sync {
	type ExtraInfo: Clone + Send + Sync + 'static;
	type ExtraHistoricInfo: Clone + Send + Sync + 'static;

	type Index: aliases::Index;
	type Hash: aliases::Hash;
	type Data: aliases::Data;

	type Client: ChainClient<Index = Self::Index, Hash = Self::Hash, Data = Self::Data>;

	type Chain: ExternalChain<ChainBlockNumber = Self::Index> + Send + Sync + 'static;

	type Parameters: Send;

	async fn stream(&self, parameters: Self::Parameters) -> BoxActiveAndFuture<'_, Item<'_, Self>>;
}

pub type Item<'a, T> = (
	Vault<
		<T as ChunkedByVault>::Chain,
		<T as ChunkedByVault>::ExtraInfo,
		<T as ChunkedByVault>::ExtraHistoricInfo,
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
		TExtraInfo: Clone + Send + Sync + 'static,
		TExtraHistoricInfo: Clone + Send + Sync + 'static,
		TChain: ExternalChain<ChainBlockNumber = T::Index>,
		T: ChunkedChainSource<
			Info = (
				<<TChain as Chain>::ChainCrypto as ChainCrypto>::AggKey,
				<TChain as Chain>::ChainBlockNumber,
				TExtraInfo,
			),
			HistoricInfo = (
				<<TChain as Chain>::ChainCrypto as ChainCrypto>::AggKey,
				<TChain as Chain>::ChainBlockNumber,
				TExtraHistoricInfo,
			),
			Chain = TChain,
		>,
	> ChunkedByVault for T
{
	type ExtraInfo = TExtraInfo;
	type ExtraHistoricInfo = TExtraHistoricInfo;

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

#[derive(Clone)]
pub struct ChunkByVault<TChainSource: ExternalChainSource, Info, HistoricInfo>
where
	state_chain_runtime::Runtime: RuntimeHasChain<TChainSource::Chain>,
{
	chain_source: TChainSource,
	_phantom: std::marker::PhantomData<(Info, HistoricInfo)>,
}
impl<TChainSource: ExternalChainSource, Info, HistoricInfo>
	ChunkByVault<TChainSource, Info, HistoricInfo>
where
	state_chain_runtime::Runtime: RuntimeHasChain<TChainSource::Chain>,
{
	pub fn new(chain_source: TChainSource) -> Self {
		Self { chain_source, _phantom: Default::default() }
	}
}
#[async_trait::async_trait]
impl<TChainSource: ExternalChainSource, ExtraInfo, ExtraHistoricInfo> ChunkedByVault
	for ChunkByVault<TChainSource, ExtraInfo, ExtraHistoricInfo>
where
	state_chain_runtime::Runtime: RuntimeHasChain<TChainSource::Chain>,
	ExtraInfo: Clone + Send + Sync + 'static,
	ExtraHistoricInfo: Clone + Send + Sync + 'static,
{
	type ExtraInfo = ExtraInfo;
	type ExtraHistoricInfo = ExtraHistoricInfo;

	type Index = TChainSource::Index;
	type Hash = TChainSource::Hash;
	type Data = TChainSource::Data;

	type Client = TChainSource::Client;

	type Chain = TChainSource::Chain;

	type Parameters = VaultSource<TChainSource::Chain, ExtraInfo, ExtraHistoricInfo>;

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
								header.index >= vault.info.1 &&
									vault
										.historic_signal
										.get()
										.map_or(true, |(_, end_index, _)| {
											header.index < *end_index
										}),
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
