use futures_core::Future;
use futures_util::StreamExt;

use crate::witness::{
	chain_source::{aliases, ChainClient, ChainStream, Header},
	common::BoxActiveAndFuture,
	epoch_source::Epoch,
};

use super::ChunkedChainSource;

pub struct Map<Inner, MapFn> {
	inner: Inner,
	map_fn: MapFn,
}
impl<Inner, MapFn> Map<Inner, MapFn> {
	pub fn new(inner: Inner, map_fn: MapFn) -> Self {
		Self { inner, map_fn }
	}
}
#[async_trait::async_trait]
impl<Inner: ChunkedChainSource, MappedTo, FutMappedTo, MapFn> ChunkedChainSource
	for Map<Inner, MapFn>
where
	MappedTo: aliases::Data,
	FutMappedTo: Future<Output = MappedTo> + Send,
	MapFn: Fn(
			Epoch<Inner::Info, Inner::HistoricInfo>,
			Header<Inner::Index, Inner::Hash, Inner::Data>,
		) -> FutMappedTo
		+ Send
		+ Sync
		+ Clone,
{
	type Info = Inner::Info;
	type HistoricInfo = Inner::HistoricInfo;

	type Index = Inner::Index;
	type Hash = Inner::Hash;
	type Data = MappedTo;

	type Client = MappedClient<Inner, MapFn>;

	type Chain = Inner::Chain;

	type Parameters = Inner::Parameters;

	async fn stream(
		&self,
		parameters: Self::Parameters,
	) -> BoxActiveAndFuture<'_, super::Item<'_, Self, Self::Info, Self::HistoricInfo>> {
		self.inner
			.stream(parameters)
			.await
			.then(move |(epoch, chain_stream, chain_client)| async move {
				(
					epoch.clone(),
					chain_stream
						.then({
							let epoch = epoch.clone();
							move |header| {
								let epoch = epoch.clone();
								async move {
									Header {
										index: header.index,
										hash: header.hash,
										parent_hash: header.parent_hash,
										data: (self.map_fn)(epoch, header).await,
									}
								}
							}
						})
						.into_box(),
					MappedClient::new(chain_client, self.map_fn.clone(), epoch),
				)
			})
			.await
			.into_box()
	}
}

pub struct MappedClient<Inner: ChunkedChainSource, MapFn> {
	inner_client: Inner::Client,
	map_fn: MapFn,
	epoch: Epoch<Inner::Info, Inner::HistoricInfo>,
}
impl<Inner: ChunkedChainSource, MapFn> MappedClient<Inner, MapFn> {
	pub fn new(
		inner_client: Inner::Client,
		map_fn: MapFn,
		epoch: Epoch<Inner::Info, Inner::HistoricInfo>,
	) -> Self {
		Self { inner_client, map_fn, epoch }
	}
}
#[async_trait::async_trait]
impl<
		Inner: ChunkedChainSource,
		MappedTo: aliases::Data,
		FutMappedTo: Future<Output = MappedTo> + Send,
		MapFn: Fn(
				Epoch<Inner::Info, Inner::HistoricInfo>,
				Header<Inner::Index, Inner::Hash, Inner::Data>,
			) -> FutMappedTo
			+ Send
			+ Sync
			+ Clone,
	> ChainClient for MappedClient<Inner, MapFn>
{
	type Index = Inner::Index;
	type Hash = Inner::Hash;
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
			data: (self.map_fn)(self.epoch.clone(), header).await,
		}
	}
}
