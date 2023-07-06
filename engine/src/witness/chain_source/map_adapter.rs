use super::{aliases, BoxChainStream, ChainSourceWithClient, Header};

use futures::Future;
use futures_util::StreamExt;

use crate::witness::chain_source::ChainClient;

pub struct MapAdapter<Underlying, MapFn> {
	underlying_stream: Underlying,
	map_fn: MapFn,
}

impl<Underlying, MapFn> MapAdapter<Underlying, MapFn> {
	pub fn new(underlying_stream: Underlying, map_fn: MapFn) -> Self {
		Self { underlying_stream, map_fn }
	}
}

#[async_trait::async_trait]
impl<
		Underlying: ChainSourceWithClient,
		MappedTo: aliases::Data,
		FutMappedTo: Future<Output = MappedTo> + Send + Sync,
		MapFn: Fn(Underlying::Index, Underlying::Hash, Underlying::Data) -> FutMappedTo
			+ Send
			+ Sync
			+ Clone,
	> ChainSourceWithClient for MapAdapter<Underlying, MapFn>
{
	type Index = Underlying::Index;
	type Hash = Underlying::Hash;
	type Data = MappedTo;

	type Client = MappedClient<Underlying::Client, MapFn>;

	async fn stream_and_client(
		&self,
	) -> (BoxChainStream<'_, Self::Index, Self::Hash, Self::Data>, Self::Client) {
		let (underlying_stream, client) = self.underlying_stream.stream_and_client().await;

		let mapped_stream =
			underlying_stream.then(move |Header { index, hash, parent_hash, data }| async move {
				Header { index, hash, parent_hash, data: (self.map_fn)(index, hash, data).await }
			});

		(Box::pin(mapped_stream), MappedClient::new(client, self.map_fn.clone()))
	}
}

pub struct MappedClient<UnderlyingClient, MapFn> {
	underlying_client: UnderlyingClient,
	map_fn: MapFn,
}

impl<UnderlyingClient, MapFn> MappedClient<UnderlyingClient, MapFn> {
	pub fn new(underlying_client: UnderlyingClient, map_fn: MapFn) -> Self {
		Self { underlying_client, map_fn }
	}
}

#[async_trait::async_trait]
impl<
		UnderlyingClient: ChainClient,
		MappedTo: aliases::Data,
		FutMappedTo: Future<Output = MappedTo> + Send + Sync,
		MapFn: Fn(
				UnderlyingClient::Index,
				UnderlyingClient::Hash,
				UnderlyingClient::Data,
			) -> FutMappedTo
			+ Send
			+ Sync
			+ Clone,
	> ChainClient for MappedClient<UnderlyingClient, MapFn>
{
	type Index = UnderlyingClient::Index;
	type Hash = UnderlyingClient::Hash;
	type Data = MappedTo;

	async fn header_at_index(
		&self,
		index: Self::Index,
	) -> Header<Self::Index, Self::Hash, Self::Data> {
		let Header { index, hash, parent_hash, data } =
			self.underlying_client.header_at_index(index).await;

		Header { index, hash, parent_hash, data: (self.map_fn)(index, hash, data).await }
	}
}
