use ethers::types::Bloom;
use sp_core::H256;

use crate::{
	eth::retry_rpc::EthersRetrySubscribeApi,
	witness::{
		common::{
			chain_source::{BoxChainStream, ChainClient, ChainSource},
			ExternalChainSource,
		},
		evm::source::inner_stream_and_client,
	},
};
use std::time::Duration;

#[derive(Clone)]
pub struct EthSource<C> {
	client: C,
}

impl<C: EthersRetrySubscribeApi + ChainClient<Index = u64, Hash = H256, Data = Bloom> + Clone>
	EthSource<C>
{
	pub fn new(client: C) -> Self {
		Self { client }
	}
}

/// The maximum amount of time we wait for a block to be pulled from the stream.
const BLOCK_PULL_TIMEOUT: Duration = Duration::from_secs(60);

#[async_trait::async_trait]
impl<C> ChainSource for EthSource<C>
where
	C: EthersRetrySubscribeApi + ChainClient<Index = u64, Hash = H256, Data = Bloom> + Clone,
{
	type Index = <C as ChainClient>::Index;
	type Hash = <C as ChainClient>::Hash;
	type Data = <C as ChainClient>::Data;
	type Client = C;

	async fn stream_and_client(
		&self,
	) -> (BoxChainStream<'_, Self::Index, Self::Hash, Self::Data>, Self::Client) {
		inner_stream_and_client(self.client.clone(), BLOCK_PULL_TIMEOUT).await
	}
}

impl<C> ExternalChainSource for EthSource<C>
where
	C: EthersRetrySubscribeApi + ChainClient<Index = u64, Hash = H256, Data = Bloom> + Clone,
{
	type Chain = cf_chains::Ethereum;
}
