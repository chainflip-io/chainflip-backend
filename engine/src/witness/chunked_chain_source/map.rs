use cf_primitives::EpochIndex;
use futures_core::Future;
use futures_util::StreamExt;

use crate::witness::{
	chain_source::{aliases, box_chain_stream, ChainClient, Header},
	common::BoxActiveAndFuture,
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
impl<'a, Inner: ChunkedChainSource<'a>, MappedTo, FutMappedTo, MapFn> ChunkedChainSource<'a>
	for Map<Inner, MapFn>
where
	Self: 'a,
	MappedTo: aliases::Data,
	FutMappedTo: Future<Output = MappedTo> + Send,
	MapFn: Fn(EpochIndex, Inner::Info, Header<Inner::Index, Inner::Hash, Inner::Data>) -> FutMappedTo
		+ Send
		+ Sync
		+ Clone,
{
	type Info = Inner::Info;
	type HistoricInfo = Inner::HistoricInfo;

	type Index = Inner::Index;
	type Hash = Inner::Hash;
	type Data = MappedTo;

	type Client = MappedClient<'a, Inner, MapFn>;

	type Chain = Inner::Chain;

	async fn stream(
		self,
	) -> BoxActiveAndFuture<'a, super::Item<'a, Self, Self::Info, Self::HistoricInfo>> {
		self.inner
			.stream()
			.await
			.then(move |(epoch, chain_stream, chain_client)| {
				let map_fn = self.map_fn.clone();
				async move {
					let epoch_index = epoch.index;
					let epoch_info = epoch.info.clone();
					(
						epoch,
						box_chain_stream(chain_stream.then({
							let map_fn = map_fn.clone();
							let epoch_info = epoch_info.clone();
							move |header| {
								let map_fn = map_fn.clone();
								let epoch_info = epoch_info.clone();
								async move {
									Header {
										index: header.index,
										hash: header.hash,
										parent_hash: header.parent_hash,
										data: (map_fn)(epoch_index, epoch_info, header).await,
									}
								}
							}
						})),
						MappedClient::new(chain_client, map_fn.clone(), epoch_index, epoch_info),
					)
				}
			})
			.await
			.into_box()
	}
}

pub struct MappedClient<'a, Inner: ChunkedChainSource<'a>, MapFn> {
	inner_client: Inner::Client,
	map_fn: MapFn,
	index: EpochIndex,
	info: Inner::Info,
}
impl<'a, Inner: ChunkedChainSource<'a>, MapFn> MappedClient<'a, Inner, MapFn> {
	pub fn new(
		inner_client: Inner::Client,
		map_fn: MapFn,
		index: EpochIndex,
		info: Inner::Info,
	) -> Self {
		Self { inner_client, map_fn, index, info }
	}
}
#[async_trait::async_trait]
impl<
		'a,
		Inner: ChunkedChainSource<'a>,
		MappedTo: aliases::Data,
		FutMappedTo: Future<Output = MappedTo> + Send,
		MapFn: Fn(
				EpochIndex,
				Inner::Info,
				Header<Inner::Index, Inner::Hash, Inner::Data>,
			) -> FutMappedTo
			+ Send
			+ Sync
			+ Clone,
	> ChainClient for MappedClient<'a, Inner, MapFn>
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
			data: (self.map_fn)(self.index, self.info.clone(), header).await,
		}
	}
}
