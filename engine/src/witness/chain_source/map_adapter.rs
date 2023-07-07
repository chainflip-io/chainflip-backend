use super::{aliases, BoxChainStream, ChainSource, Header};

use futures::Future;
use futures_util::StreamExt;

use crate::witness::chain_source::ChainClient;

pub struct MapAdapter<InnerSource, MapFn> {
	inner_source: InnerSource,
	map_fn: MapFn,
}

impl<InnerSource, MapFn> MapAdapter<InnerSource, MapFn> {
	pub fn new(inner_source: InnerSource, map_fn: MapFn) -> Self {
		Self { inner_source, map_fn }
	}
}

#[async_trait::async_trait]
impl<
		InnerSource: ChainSource,
		MappedTo: aliases::Data,
		FutMappedTo: Future<Output = MappedTo> + Send + Sync,
		MapFn: Fn(Header<InnerSource::Index, InnerSource::Hash, InnerSource::Data>) -> FutMappedTo
			+ Send
			+ Sync
			+ Clone,
	> ChainSource for MapAdapter<InnerSource, MapFn>
{
	type Index = InnerSource::Index;
	type Hash = InnerSource::Hash;
	type Data = MappedTo;

	type Client = MappedClient<InnerSource::Client, MapFn>;

	async fn stream_and_client(
		&self,
	) -> (BoxChainStream<'_, Self::Index, Self::Hash, Self::Data>, Self::Client) {
		let (inner_stream, inner_client) = self.inner_source.stream_and_client().await;

		let mapped_stream = inner_stream.then(move |header| async move {
			Header {
				index: header.index,
				hash: header.hash,
				parent_hash: header.parent_hash,
				data: (self.map_fn)(header).await,
			}
		});

		(Box::pin(mapped_stream), MappedClient::new(inner_client, self.map_fn.clone()))
	}
}

pub struct MappedClient<InnerClient, MapFn> {
	inner_client: InnerClient,
	map_fn: MapFn,
}

impl<InnerClient, MapFn> MappedClient<InnerClient, MapFn> {
	pub fn new(inner_client: InnerClient, map_fn: MapFn) -> Self {
		Self { inner_client, map_fn }
	}
}

#[async_trait::async_trait]
impl<
		InnerClient: ChainClient,
		MappedTo: aliases::Data,
		FutMappedTo: Future<Output = MappedTo> + Send + Sync,
		MapFn: Fn(Header<InnerClient::Index, InnerClient::Hash, InnerClient::Data>) -> FutMappedTo
			+ Send
			+ Sync
			+ Clone,
	> ChainClient for MappedClient<InnerClient, MapFn>
{
	type Index = InnerClient::Index;
	type Hash = InnerClient::Hash;
	type Data = MappedTo;

	async fn header_at_index(
		&self,
		index: Self::Index,
	) -> Header<Self::Index, Self::Hash, Self::Data> {
		let header = self.inner_client.header_at_index(index).await;

		Header {
			index: header.index,
			hash: header.hash,
			parent_hash: header.parent_hash,
			data: (self.map_fn)(header).await,
		}
	}
}
