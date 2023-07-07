use futures_core::Future;
use futures_util::StreamExt;

use crate::witness::{
	chain_source::{aliases, box_chain_stream, map::MappedClient, Header},
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
	MapFn: Fn(Header<Inner::Index, Inner::Hash, Inner::Data>) -> FutMappedTo + Send + Sync + Clone,
{
	type Info = Inner::Info;
	type HistoricInfo = Inner::HistoricInfo;

	type Index = Inner::Index;
	type Hash = Inner::Hash;
	type Data = MappedTo;

	type Client = MappedClient<Inner::Client, MapFn>;

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
					(
						epoch,
						box_chain_stream(chain_stream.then({
							let map_fn = map_fn.clone();
							move |header| {
								let map_fn = map_fn.clone();
								async move {
									Header {
										index: header.index,
										hash: header.hash,
										parent_hash: header.parent_hash,
										data: (map_fn)(header).await,
									}
								}
							}
						})),
						MappedClient::new(chain_client, map_fn.clone()),
					)
				}
			})
			.await
			.into_box()
	}
}
