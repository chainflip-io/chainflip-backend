use cf_chains::dot::PolkadotHash;
use cf_primitives::PolkadotBlockNumber;

use crate::dot::rpc::DotSubscribeApi;
use futures::stream::StreamExt;

use super::{BoxChainStream, ChainSource, Header};
use subxt::config::Header as SubxtHeader;

pub struct DotUnfinalisedSource<C: DotSubscribeApi> {
	client: C,
}

impl<C: DotSubscribeApi> DotUnfinalisedSource<C> {
	pub fn new(client: C) -> Self {
		Self { client }
	}
}

#[async_trait::async_trait]
impl<C: DotSubscribeApi + Clone> ChainSource for DotUnfinalisedSource<C> {
	type Index = PolkadotBlockNumber;
	type Hash = PolkadotHash;
	type Data = ();

	async fn stream(&self) -> BoxChainStream<'_, Self::Index, Self::Hash, Self::Data> {
		let mut client = self.client.clone();
		Box::pin(client.subscribe_best_heads().await.unwrap().map(|header| {
			let header = header.unwrap();
			Header {
				index: header.number,
				hash: header.hash(),
				parent_hash: Some(header.parent_hash),
				data: (),
			}
		}))
	}
}
