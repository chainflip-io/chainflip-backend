use sp_core::H256;

use crate::eth::ethers_rpc::EthersSubscribeApi;
use futures::stream::StreamExt;

use super::{BoxChainStream, ChainSource, Header};

pub struct EthSource<C: EthersSubscribeApi> {
	client: C,
}

impl<C: EthersSubscribeApi> EthSource<C> {
	pub fn new(client: C) -> Self {
		Self { client }
	}
}

#[async_trait::async_trait]
impl<C: EthersSubscribeApi + Send + Sync> ChainSource for EthSource<C> {
	type Index = u64;
	type Hash = H256;
	type Data = ();

	async fn stream(&self) -> BoxChainStream<'_, Self::Index, Self::Hash, Self::Data> {
		Box::pin(self.client.subscribe_blocks().await.unwrap().map(|block| Header {
			index: block.number.unwrap().as_u64(),
			hash: block.hash.unwrap(),
			parent_hash: Some(block.parent_hash),
			data: (),
		}))
	}
}
