use std::{pin::Pin, time::Duration};

use cf_chains::dot::PolkadotHash;
use cf_primitives::PolkadotBlockNumber;
use futures_util::stream;
use subxt::{events::Events, PolkadotConfig};

use crate::dot::{
	retry_rpc::{DotRetryRpcApi, DotRetrySubscribeApi},
	rpc::PolkadotHeader,
};
use futures::{stream::StreamExt, Stream};

use super::{BoxChainStream, ChainSource, Header};
use subxt::config::Header as SubxtHeader;

use anyhow::Result;

pub struct DotUnfinalisedSource<C: DotRetrySubscribeApi + DotRetryRpcApi> {
	client: C,
}

impl<C: DotRetrySubscribeApi + DotRetryRpcApi> DotUnfinalisedSource<C> {
	pub fn new(client: C) -> Self {
		Self { client }
	}
}

const TIMEOUT: Duration = Duration::from_secs(20);
const RESTART_STREAM_DELAY: Duration = Duration::from_secs(6);

#[async_trait::async_trait]
impl<C: DotRetrySubscribeApi + DotRetryRpcApi + Send + Sync + Clone> ChainSource
	for DotUnfinalisedSource<C>
{
	type Index = PolkadotBlockNumber;
	type Hash = PolkadotHash;
	type Data = Events<PolkadotConfig>;

	async fn stream(&self) -> BoxChainStream<'_, Self::Index, Self::Hash, Self::Data> {
		pub struct State<C> {
			client: C,
			stream: Pin<Box<dyn Stream<Item = Result<PolkadotHeader>> + Send>>,
		}
		let mut client = self.client.clone();

		let stream = client.subscribe_best_heads().await;
		Box::pin(stream::unfold(State { client, stream }, |mut state| async move {
			loop {
				while let Ok(Some(header)) =
					tokio::time::timeout(TIMEOUT, state.stream.next()).await
				{
					if let Ok(header) = header {
						let Some(events) = state.client.events(header.hash()).await else {
							continue;
						};

						return Some((
							Header {
								index: header.number,
								hash: header.hash(),
								parent_hash: Some(header.parent_hash),
								data: events,
							},
							state,
						))
					}
				}
				// We don't want to spam retries if the node returns a stream that's empty
				// immediately.
				tokio::time::sleep(RESTART_STREAM_DELAY).await;
				let stream = state.client.subscribe_best_heads().await;
				state = State { client: state.client, stream };
			}
		}))
	}
}
