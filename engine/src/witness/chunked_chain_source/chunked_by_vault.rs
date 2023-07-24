pub mod builder;
pub mod continuous;
pub mod egress_items;
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
	type Info: Clone + Send + Sync + 'static;
	type HistoricInfo: Clone + Send + Sync + 'static;

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
		(pallet_cf_vaults::Vault<<T as ChunkedByVault>::Chain>, <T as ChunkedByVault>::Info),
		(
			<<T as ChunkedByVault>::Chain as Chain>::ChainBlockNumber,
			<T as ChunkedByVault>::HistoricInfo,
		),
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
			Info = (pallet_cf_vaults::Vault<TChain>, TExtraInfo),
			HistoricInfo = (<TChain as Chain>::ChainBlockNumber, TExtraHistoricInfo),
			Chain = TChain,
		>,
	> ChunkedByVault for T
{
	type Info = TExtraInfo;
	type HistoricInfo = TExtraHistoricInfo;

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
impl<TChainSource: ExternalChainSource, Info, HistoricInfo> ChunkedByVault
	for ChunkByVault<TChainSource, Info, HistoricInfo>
where
	state_chain_runtime::Runtime: RuntimeHasChain<TChainSource::Chain>,
	Info: Clone + Send + Sync + 'static,
	HistoricInfo: Clone + Send + Sync + 'static,
{
	type Info = Info;
	type HistoricInfo = HistoricInfo;

	type Index = TChainSource::Index;
	type Hash = TChainSource::Hash;
	type Data = TChainSource::Data;

	type Client = TChainSource::Client;

	type Chain = TChainSource::Chain;

	type Parameters = VaultSource<TChainSource::Chain, Info, HistoricInfo>;

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
								header.index >= vault.info.0.active_from_block &&
									vault
										.historic_signal
										.get()
										.map_or(true, |(end_index, _)| {
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
