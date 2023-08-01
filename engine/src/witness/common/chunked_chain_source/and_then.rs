use futures_core::Future;
use futures_util::StreamExt;

use crate::witness::common::{
	chain_source::{aliases, ChainClient, ChainStream, Header},
	epoch_source::Epoch,
	BoxActiveAndFuture,
};

use super::ChunkedChainSource;

pub struct AndThen<Inner, F> {
	inner: Inner,
	f: F,
}
impl<Inner, F> AndThen<Inner, F> {
	pub fn new(inner: Inner, f: F) -> Self {
		Self { inner, f }
	}
}
#[async_trait::async_trait]
impl<Inner, Input, Output, Error, Fut, F> ChunkedChainSource for AndThen<Inner, F>
where
	Input: aliases::Data,
	Output: aliases::Data,
	Error: aliases::Data,
	Inner: ChunkedChainSource<Data = Result<Input, Error>>,
	Fut: Future<Output = Result<Output, Error>> + Send,
	F: Fn(Epoch<Inner::Info, Inner::HistoricInfo>, Header<Inner::Index, Inner::Hash, Input>) -> Fut
		+ Send
		+ Sync
		+ Clone,
{
	type Info = Inner::Info;
	type HistoricInfo = Inner::HistoricInfo;

	type Index = Inner::Index;
	type Hash = Inner::Hash;
	type Data = Fut::Output;

	type Client = AndThenClient<Inner, F>;

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
								#[allow(clippy::redundant_async_block)]
								header.and_then_data(move |header| async move {
									(self.f)(epoch, header).await
								})
							}
						})
						.into_box(),
					AndThenClient::new(chain_client, self.f.clone(), epoch),
				)
			})
			.await
			.into_box()
	}
}

pub struct AndThenClient<Inner: ChunkedChainSource, F> {
	inner_client: Inner::Client,
	f: F,
	epoch: Epoch<Inner::Info, Inner::HistoricInfo>,
}

impl<Inner: ChunkedChainSource, F: Clone> Clone for AndThenClient<Inner, F> {
	fn clone(&self) -> Self {
		Self {
			inner_client: self.inner_client.clone(),
			f: self.f.clone(),
			epoch: self.epoch.clone(),
		}
	}
}

impl<Inner: ChunkedChainSource, F> AndThenClient<Inner, F> {
	pub fn new(
		inner_client: Inner::Client,
		f: F,
		epoch: Epoch<Inner::Info, Inner::HistoricInfo>,
	) -> Self {
		Self { inner_client, f, epoch }
	}
}
#[async_trait::async_trait]
impl<Input, Output, Error, Inner, Fut, F> ChainClient for AndThenClient<Inner, F>
where
	Input: aliases::Data,
	Output: aliases::Data,
	Error: aliases::Data,
	Inner: ChunkedChainSource<Data = Result<Input, Error>>,
	Fut: Future<Output = Result<Output, Error>> + Send,
	F: Fn(Epoch<Inner::Info, Inner::HistoricInfo>, Header<Inner::Index, Inner::Hash, Input>) -> Fut
		+ Send
		+ Sync
		+ Clone,
{
	type Index = Inner::Index;
	type Hash = Inner::Hash;
	type Data = Result<Output, Error>;

	async fn header_at_index(
		&self,
		index: Self::Index,
	) -> Header<Self::Index, Self::Hash, Self::Data> {
		self.inner_client
			.header_at_index(index)
			.await
			.and_then_data(move |header| (self.f)(self.epoch.clone(), header))
			.await
	}
}
