use ethers::types::Bloom;
use sp_core::H256;

use crate::{
	eth::{
		core_h256, retry_rpc::EthersRetrySubscribeApi, ConscientiousEthWebsocketBlockHeaderStream,
	},
	witness::{
		chain_source::{ChainClient, ChainSource},
		common::ExternalChainSource,
	},
};
use futures::stream::StreamExt;
use futures_util::stream;
use std::time::Duration;

use super::{BoxChainStream, Header};

pub struct EthSource<C> {
	client: C,
}

impl<C> EthSource<C> {
	pub fn new(client: C) -> Self {
		Self { client }
	}
}

const TIMEOUT: Duration = Duration::from_secs(60);
const RESTART_STREAM_DELAY: Duration = Duration::from_secs(6);

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
		pub struct State<C> {
			client: C,
			stream: ConscientiousEthWebsocketBlockHeaderStream,
		}

		let client = self.client.clone();
		let stream = client.subscribe_blocks().await;
		(
			Box::pin(stream::unfold(State { client, stream }, |mut state| async move {
				loop {
					while let Ok(Some(header)) =
						tokio::time::timeout(TIMEOUT, state.stream.next()).await
					{
						if let Ok(header) = header {
							if header.number.is_none() || header.hash.is_none() {
								continue
							}

							return Some((
								Header {
									index: header.number.unwrap().as_u64(),
									hash: core_h256(header.hash.unwrap()),
									parent_hash: Some(core_h256(header.parent_hash)),
									data: header.logs_bloom.0.into(),
								},
								state,
							))
						}
					}

					// We don't want to spam retries if the node returns a stream that's empty
					// immediately.
					tokio::time::sleep(RESTART_STREAM_DELAY).await;
					let stream = state.client.subscribe_blocks().await;
					state = State { client: state.client, stream };
				}
			})),
			self.client.clone(),
		)
	}
}

impl<C> ExternalChainSource for EthSource<C>
where
	C: EthersRetrySubscribeApi + ChainClient<Index = u64, Hash = H256, Data = Bloom> + Clone,
{
	type Chain = cf_chains::Ethereum;
}
