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

use super::{BoxChainStream, ChainClient, ChainSourceWithClient, Header};
use subxt::config::Header as SubxtHeader;

use anyhow::Result;

#[async_trait::async_trait]
pub trait GetPolkadotStream {
	async fn get_stream(
		&self,
	) -> Pin<Box<dyn Stream<Item = anyhow::Result<PolkadotHeader>> + Send>>;
}

pub struct DotUnfinalisedSource<C: DotRetrySubscribeApi + DotRetryRpcApi> {
	client: C,
}

#[async_trait::async_trait]
impl<C: DotRetrySubscribeApi + DotRetryRpcApi + Send + Sync> GetPolkadotStream
	for DotUnfinalisedSource<C>
{
	async fn get_stream(
		&self,
	) -> Pin<Box<dyn Stream<Item = anyhow::Result<PolkadotHeader>> + Send>> {
		self.client.subscribe_best_heads().await
	}
}

impl<C: DotRetrySubscribeApi + DotRetryRpcApi + Send> DotUnfinalisedSource<C> {
	pub fn new(client: C) -> Self {
		Self { client }
	}
}

const TIMEOUT: Duration = Duration::from_secs(20);
const RESTART_STREAM_DELAY: Duration = Duration::from_secs(6);

#[async_trait::async_trait]
impl<C> ChainSourceWithClient for DotUnfinalisedSource<C>
where
	C: ChainClient<Index = PolkadotBlockNumber, Hash = PolkadotHash, Data = Events<PolkadotConfig>>
		+ GetPolkadotStream
		+ DotRetryRpcApi
		+ DotRetrySubscribeApi
		+ Clone
		+ 'static,
{
	type Index = <C as ChainClient>::Index;
	type Hash = <C as ChainClient>::Hash;
	type Data = <C as ChainClient>::Data;
	type Client = C;

	async fn stream_and_client(
		&self,
	) -> (BoxChainStream<'_, Self::Index, Self::Hash, Self::Data>, Self::Client) {
		(polkadot_source(self.client.clone()).await, self.client.clone())
	}
}

pub struct DotFinalisedSource<C: DotRetrySubscribeApi + DotRetryRpcApi> {
	client: C,
}

impl<C: DotRetrySubscribeApi + DotRetryRpcApi + Send> DotFinalisedSource<C> {
	pub fn new(client: C) -> Self {
		Self { client }
	}
}

#[async_trait::async_trait]
impl<C: DotRetrySubscribeApi + DotRetryRpcApi + Send + Sync> GetPolkadotStream
	for DotFinalisedSource<C>
{
	async fn get_stream(
		&self,
	) -> Pin<Box<dyn Stream<Item = anyhow::Result<PolkadotHeader>> + Send>> {
		self.client.subscribe_finalized_heads().await
	}
}

#[async_trait::async_trait]
impl<
		C: ChainClient<
				Index = PolkadotBlockNumber,
				Hash = PolkadotHash,
				Data = Events<PolkadotConfig>,
			> + GetPolkadotStream
			+ DotRetryRpcApi
			+ DotRetrySubscribeApi
			+ Clone
			+ 'static,
	> ChainSourceWithClient for DotFinalisedSource<C>
{
	type Index = <C as ChainClient>::Index;
	type Hash = <C as ChainClient>::Hash;
	type Data = <C as ChainClient>::Data;
	type Client = C;

	async fn stream_and_client(
		&self,
	) -> (BoxChainStream<'_, Self::Index, Self::Hash, Self::Data>, Self::Client) {
		(polkadot_source(self.client.clone()).await, self.client.clone())
	}
}

async fn polkadot_source<
	'a,
	C: ChainClient<Index = PolkadotBlockNumber, Hash = PolkadotHash, Data = Events<PolkadotConfig>>
		+ GetPolkadotStream
		+ DotRetrySubscribeApi
		+ DotRetryRpcApi
		+ Clone
		+ 'static,
>(
	client: C,
) -> BoxChainStream<'a, C::Index, C::Hash, C::Data> {
	pub struct State<C> {
		client: C,
		stream: Pin<Box<dyn Stream<Item = Result<PolkadotHeader>> + Send>>,
	}
	let client = client.clone();
	let stream = client.get_stream().await;
	Box::pin(stream::unfold(State { client, stream }, |mut state| async move {
		loop {
			while let Ok(Some(header)) = tokio::time::timeout(TIMEOUT, state.stream.next()).await {
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
			let stream = state.client.get_stream().await;
			state = State { client: state.client, stream };
		}
	}))
}
