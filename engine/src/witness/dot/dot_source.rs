use std::{pin::Pin, time::Duration};

use cf_chains::dot::PolkadotHash;
use cf_primitives::PolkadotBlockNumber;
use futures_util::stream;
use subxt::{events::Events, PolkadotConfig};

use crate::{
	dot::{
		retry_rpc::{DotRetryRpcApi, DotRetrySubscribeApi},
		rpc::PolkadotHeader,
	},
	witness::common::{
		chain_source::{BoxChainStream, ChainClient, ChainSource, Header},
		ExternalChainSource,
	},
};
use futures::{stream::StreamExt, Stream};

use anyhow::Result;
use subxt::{self, config::Header as SubxtHeader};

macro_rules! polkadot_source {
	($self:expr, $func:ident) => {{
		struct State<C> {
			client: C,
			stream: Pin<Box<dyn Stream<Item = Result<PolkadotHeader>> + Send>>,
		}

		let client = $self.client.clone();
		let stream = client.$func().await;

		(
			Box::pin(stream::unfold(State { client, stream }, |mut state| async move {
				loop {
					while let Ok(Some(header)) =
						tokio::time::timeout(TIMEOUT, state.stream.next()).await
					{
						if let Ok(header) = header {
							let Some(events) = state.client.events(header.hash()).await else {
								continue
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
					tracing::warn!(
						"Timeout getting next header from Polkadot {} stream. Restarting stream...",
						stringify!($func)
					);
					tokio::time::sleep(RESTART_STREAM_DELAY).await;
					let stream = state.client.$func().await;
					state = State { client: state.client, stream };
				}
			})),
			$self.client.clone(),
		)
	}};
}

#[derive(Clone)]
pub struct DotUnfinalisedSource<C> {
	client: C,
}

impl<C> DotUnfinalisedSource<C> {
	pub fn new(client: C) -> Self {
		Self { client }
	}
}

const TIMEOUT: Duration = Duration::from_secs(20);
const RESTART_STREAM_DELAY: Duration = Duration::from_secs(6);

#[async_trait::async_trait]
impl<C> ChainSource for DotUnfinalisedSource<C>
where
	C: ChainClient<Index = PolkadotBlockNumber, Hash = PolkadotHash, Data = Events<PolkadotConfig>>
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
		polkadot_source!(self, subscribe_best_heads)
	}
}

impl<
		C: ChainClient<
				Index = PolkadotBlockNumber,
				Hash = PolkadotHash,
				Data = Events<PolkadotConfig>,
			> + DotRetryRpcApi
			+ DotRetrySubscribeApi
			+ Clone
			+ 'static,
	> ExternalChainSource for DotUnfinalisedSource<C>
{
	type Chain = cf_chains::Polkadot;
}

pub struct DotFinalisedSource<C> {
	client: C,
}

impl<C> DotFinalisedSource<C> {
	pub fn new(client: C) -> Self {
		Self { client }
	}
}

#[async_trait::async_trait]
impl<
		C: ChainClient<
				Index = PolkadotBlockNumber,
				Hash = PolkadotHash,
				Data = Events<PolkadotConfig>,
			> + DotRetryRpcApi
			+ DotRetrySubscribeApi
			+ Clone
			+ 'static,
	> ChainSource for DotFinalisedSource<C>
{
	type Index = <C as ChainClient>::Index;
	type Hash = <C as ChainClient>::Hash;
	type Data = <C as ChainClient>::Data;
	type Client = C;

	async fn stream_and_client(
		&self,
	) -> (BoxChainStream<'_, Self::Index, Self::Hash, Self::Data>, Self::Client) {
		polkadot_source!(self, subscribe_finalized_heads)
	}
}

impl<
		C: ChainClient<
				Index = PolkadotBlockNumber,
				Hash = PolkadotHash,
				Data = Events<PolkadotConfig>,
			> + DotRetryRpcApi
			+ DotRetrySubscribeApi
			+ Clone
			+ 'static,
	> ExternalChainSource for DotFinalisedSource<C>
{
	type Chain = cf_chains::Polkadot;
}
