use super::{aliases, BoxChainStream, ChainSource, Header};

use futures::Future;
use futures_util::StreamExt;

use crate::witness::chain_source::ChainClient;

pub struct MapAdapter<Inner, MapFn> {
	inner_stream: Inner,
	map_fn: MapFn,
}

impl<Inner, MapFn> MapAdapter<Inner, MapFn> {
	pub fn new(inner_stream: Inner, map_fn: MapFn) -> Self {
		Self { inner_stream, map_fn }
	}
}

#[async_trait::async_trait]
impl<
		Inner: ChainSource,
		MappedTo: aliases::Data,
		FutMappedTo: Future<Output = MappedTo> + Send + Sync,
		MapFn: Fn(Header<Inner::Index, Inner::Hash, Inner::Data>) -> FutMappedTo + Send + Sync + Clone,
	> ChainSource for MapAdapter<Inner, MapFn>
{
	type Index = Inner::Index;
	type Hash = Inner::Hash;
	type Data = MappedTo;

	type Client = MappedClient<Inner::Client, MapFn>;

	async fn stream_and_client(
		&self,
	) -> (BoxChainStream<'_, Self::Index, Self::Hash, Self::Data>, Self::Client) {
		let (inner_stream, client) = self.inner_stream.stream_and_client().await;

		let mapped_stream = inner_stream.then(move |header| async move {
			Header {
				index: header.index,
				hash: header.hash,
				parent_hash: header.parent_hash,
				data: (self.map_fn)(header).await,
			}
		});

		(Box::pin(mapped_stream), MappedClient::new(client, self.map_fn.clone()))
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
