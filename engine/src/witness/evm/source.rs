use ethers::types::Bloom;
use sp_core::H256;

use crate::{
	eth::{
		core_h256, retry_rpc::EthersRetrySubscribeApi, ConscientiousEthWebsocketBlockHeaderStream,
	},
	witness::common::chain_source::{BoxChainStream, ChainClient, Header},
};
use futures::stream::StreamExt;
use futures_util::stream;
use std::time::Duration;

/// The time we wait before restarting the stream if we didn't get a block.
const RESTART_STREAM_DELAY: Duration = Duration::from_secs(6);

pub async fn inner_stream_and_client<'a, C>(
	client: C,
	// The maximum amount of time we wait for a block to be pulled from the stream.
	block_pull_timeout: Duration,
) -> (BoxChainStream<'a, u64, H256, Bloom>, C)
where
	C: EthersRetrySubscribeApi + ChainClient<Index = u64, Hash = H256, Data = Bloom> + Clone + 'a,
{
	pub struct State<C> {
		client: C,
		stream: ConscientiousEthWebsocketBlockHeaderStream,
	}

	let client_c = client.clone();
	let stream = client.subscribe_blocks().await;
	(
		Box::pin(stream::unfold(State { client: client_c, stream }, move |mut state| async move {
			loop {
				while let Ok(Some(header)) =
					tokio::time::timeout(block_pull_timeout, state.stream.next()).await
				{
					if let Ok(header) = header {
						let (Some(index), Some(hash)) = (header.number, header.hash) else {
								continue;
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
		client.clone(),
	)
}
