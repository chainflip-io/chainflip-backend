use super::{aliases, BoxChainStream, ChainSource, Header};

use futures::Future;
use futures_util::StreamExt;

use crate::witness::{chain_source::ChainClient, common::ExternalChainSource};

pub struct Then<InnerSource, ThenFn> {
	inner_source: InnerSource,
	then_fn: ThenFn,
}

impl<InnerSource, ThenFn> Then<InnerSource, ThenFn> {
	pub fn new(inner_source: InnerSource, then_fn: ThenFn) -> Self {
		Self { inner_source, then_fn }
	}
}

#[async_trait::async_trait]
impl<
		InnerSource: ChainSource,
		Output: aliases::Data,
		Fut: Future<Output = Output> + Send,
		ThenFn: Fn(Header<InnerSource::Index, InnerSource::Hash, InnerSource::Data>) -> Fut
			+ Send
			+ Sync
			+ Clone,
	> ChainSource for Then<InnerSource, ThenFn>
{
	type Index = InnerSource::Index;
	type Hash = InnerSource::Hash;
	type Data = Output;

	type Client = MappedClient<InnerSource::Client, ThenFn>;

	async fn stream_and_client(
		&self,
	) -> (BoxChainStream<'_, Self::Index, Self::Hash, Self::Data>, Self::Client) {
		let (inner_stream, inner_client) = self.inner_source.stream_and_client().await;

		let mapped_stream = inner_stream.then(move |header| async move {
			Header {
				index: header.index,
				hash: header.hash,
				parent_hash: header.parent_hash,
				data: (self.then_fn)(header).await,
			}
		});

		(Box::pin(mapped_stream), MappedClient::new(inner_client, self.then_fn.clone()))
	}
}

impl<
		InnerSource: ExternalChainSource,
		Output: aliases::Data,
		Fut: Future<Output = Output> + Send,
		ThenFn: Fn(Header<InnerSource::Index, InnerSource::Hash, InnerSource::Data>) -> Fut
			+ Send
			+ Sync
			+ Clone,
	> ExternalChainSource for Then<InnerSource, ThenFn>
{
	type Chain = InnerSource::Chain;
}

#[derive(Clone)]
pub struct MappedClient<InnerClient, ThenFn> {
	inner_client: InnerClient,
	then_fn: ThenFn,
}

impl<InnerClient, ThenFn> MappedClient<InnerClient, ThenFn> {
	pub fn new(inner_client: InnerClient, then_fn: ThenFn) -> Self {
		Self { inner_client, then_fn }
	}
}

#[async_trait::async_trait]
impl<
		InnerClient: ChainClient,
		Output: aliases::Data,
		Fut: Future<Output = Output> + Send,
		ThenFn: Fn(Header<InnerClient::Index, InnerClient::Hash, InnerClient::Data>) -> Fut
			+ Send
			+ Sync
			+ Clone,
	> ChainClient for MappedClient<InnerClient, ThenFn>
{
	type Index = InnerClient::Index;
	type Hash = InnerClient::Hash;
	type Data = Output;

	async fn header_at_index(
		&self,
		index: Self::Index,
	) -> Header<Self::Index, Self::Hash, Self::Data> {
		let header = self.inner_client.header_at_index(index).await;

		Header {
			index: header.index,
			hash: header.hash,
			parent_hash: header.parent_hash,
			data: (self.then_fn)(header).await,
		}
	}
}
