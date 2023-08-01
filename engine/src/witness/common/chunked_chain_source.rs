pub mod and_then;
pub mod chunked_by_time;
pub mod chunked_by_vault;
pub mod latest_then;
pub mod then;

use super::{
	chain_source::{aliases, BoxChainStream, ChainClient},
	epoch_source::Epoch,
	BoxActiveAndFuture, ExternalChain,
};

#[async_trait::async_trait]
pub trait ChunkedChainSource: Sized + Send + Sync {
	type Info: Clone + Send + Sync + 'static;
	type HistoricInfo: Clone + Send + Sync + 'static;

	type Index: aliases::Index;
	type Hash: aliases::Hash;
	type Data: aliases::Data;

	type Client: ChainClient<Index = Self::Index, Hash = Self::Hash, Data = Self::Data>;

	type Chain: ExternalChain<ChainBlockNumber = Self::Index>;

	type Parameters: Send;

	async fn stream(
		&self,
		parameters: Self::Parameters,
	) -> BoxActiveAndFuture<'_, Item<'_, Self, Self::Info, Self::HistoricInfo>>;
}

pub type Item<'a, T, Info, HistoricInfo> = (
	Epoch<Info, HistoricInfo>,
	BoxChainStream<
		'a,
		<T as ChunkedChainSource>::Index,
		<T as ChunkedChainSource>::Hash,
		<T as ChunkedChainSource>::Data,
	>,
	<T as ChunkedChainSource>::Client,
);
