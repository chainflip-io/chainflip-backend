use futures_core::Future;
use futures_util::StreamExt;

use crate::witness::common::{
	chain_source::{aliases, ChainClient, ChainStream, Header},
	epoch_source::Epoch,
	BoxActiveAndFuture,
};

use super::ChunkedChainSource;

pub struct Then<Inner, ThenFn> {
	inner: Inner,
	then_fn: ThenFn,
}
impl<Inner, ThenFn> Then<Inner, ThenFn> {
	pub fn new(inner: Inner, then_fn: ThenFn) -> Self {
		Self { inner, then_fn }
	}
}
#[async_trait::async_trait]
impl<Inner: ChunkedChainSource, Output, Fut, ThenFn> ChunkedChainSource for Then<Inner, ThenFn>
where
	Output: aliases::Data,
	Fut: Future<Output = Output> + Send,
	ThenFn: Fn(
			Epoch<Inner::Info, Inner::HistoricInfo>,
			Header<Inner::Index, Inner::Hash, Inner::Data>,
		) -> Fut
		+ Send
		+ Sync
		+ Clone,
{
	type Info = Inner::Info;
	type HistoricInfo = Inner::HistoricInfo;

	type Index = Inner::Index;
	type Hash = Inner::Hash;
	type Data = Output;

	type Client = ThenClient<Inner, ThenFn>;

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
										data: (self.then_fn)(epoch, header).await,
									}
								}
							}
						})
						.into_box(),
					ThenClient::new(chain_client, self.then_fn.clone(), epoch),
				)
			})
			.await
			.into_box()
	}
}

pub struct ThenClient<Inner: ChunkedChainSource, ThenFn> {
	inner_client: Inner::Client,
	then_fn: ThenFn,
	epoch: Epoch<Inner::Info, Inner::HistoricInfo>,
}

impl<Inner: ChunkedChainSource, ThenFn: Clone> Clone for ThenClient<Inner, ThenFn> {
	fn clone(&self) -> Self {
		Self {
			inner_client: self.inner_client.clone(),
			then_fn: self.then_fn.clone(),
			epoch: self.epoch.clone(),
		}
	}
}

impl<Inner: ChunkedChainSource, ThenFn> ThenClient<Inner, ThenFn> {
	pub fn new(
		inner_client: Inner::Client,
		then_fn: ThenFn,
		epoch: Epoch<Inner::Info, Inner::HistoricInfo>,
	) -> Self {
		Self { inner_client, then_fn, epoch }
	}
}
#[async_trait::async_trait]
impl<
		Inner: ChunkedChainSource,
		Output: aliases::Data,
		Fut: Future<Output = Output> + Send,
		ThenFn: Fn(
				Epoch<Inner::Info, Inner::HistoricInfo>,
				Header<Inner::Index, Inner::Hash, Inner::Data>,
			) -> Fut
			+ Send
			+ Sync
			+ Clone,
	> ChainClient for ThenClient<Inner, ThenFn>
{
	type Index = Inner::Index;
	type Hash = Inner::Hash;
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
			data: (self.then_fn)(self.epoch.clone(), header).await,
		}
	}
}
