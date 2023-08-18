use ethers::types::Bloom;
use futures::stream::StreamExt;
use futures_util::stream;
use sp_core::H256;

use crate::{
	eth::{
		core_h256, retry_rpc::EthersRetrySubscribeApi, ConscientiousEthWebsocketBlockHeaderStream,
	},
	witness::common::{
		chain_source::{BoxChainStream, ChainClient, ChainSource, Header},
		ExternalChain, ExternalChainSource,
	},
};
use std::time::Duration;

#[derive(Clone)]
pub struct EvmSource<Client, EvmChain> {
	client: Client,
	_phantom: std::marker::PhantomData<EvmChain>,
}

impl<
		C: EthersRetrySubscribeApi + ChainClient<Index = u64, Hash = H256, Data = Bloom> + Clone,
		EvmChain: ExternalChain,
	> EvmSource<C, EvmChain>
{
	pub fn new(client: C) -> Self {
		Self { client, _phantom: std::marker::PhantomData }
	}
}

/// The maximum amount of time we wait for a block to be pulled from the stream.
const BLOCK_PULL_TIMEOUT: Duration = Duration::from_secs(60);

/// The time we wait before restarting the stream if we didn't get a block.
const RESTART_STREAM_DELAY: Duration = Duration::from_secs(6);

#[async_trait::async_trait]
impl<C, EvmChain> ChainSource for EvmSource<C, EvmChain>
where
	C: EthersRetrySubscribeApi + ChainClient<Index = u64, Hash = H256, Data = Bloom> + Clone,
	EvmChain: ExternalChain,
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
			Box::pin(stream::unfold(State { client, stream }, move |mut state| async move {
				loop {
					while let Ok(Some(header)) =
						tokio::time::timeout(BLOCK_PULL_TIMEOUT, state.stream.next()).await
					{
						if let Ok(header) = header {
							let (Some(index), Some(hash)) = (header.number, header.hash) else {
								continue
							};

							return Some((
								Header {
									index: index.as_u64(),
									hash: core_h256(hash),
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

impl<C, EvmChain> ExternalChainSource for EvmSource<C, EvmChain>
where
	C: EthersRetrySubscribeApi + ChainClient<Index = u64, Hash = H256, Data = Bloom> + Clone,
	EvmChain: ExternalChain<ChainBlockNumber = u64>,
{
	type Chain = EvmChain;
}
