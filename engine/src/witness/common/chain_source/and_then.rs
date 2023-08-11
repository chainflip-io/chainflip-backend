use super::{aliases, BoxChainStream, ChainSource, ChainStream, Header};

use futures::Future;
use futures_util::StreamExt;

use crate::witness::common::{chain_source::ChainClient, ExternalChainSource};

pub struct AndThen<InnerSource, F> {
	inner_source: InnerSource,
	f: F,
}

impl<InnerSource, F> AndThen<InnerSource, F> {
	pub fn new(inner_source: InnerSource, f: F) -> Self {
		Self { inner_source, f }
	}
}

#[async_trait::async_trait]
impl<
		Input: aliases::Data,
		Output: aliases::Data,
		Error: aliases::Data,
		InnerSource: ChainSource<Data = Result<Input, Error>>,
		Fut: Future<Output = Result<Output, Error>> + Send,
		F: Fn(Header<InnerSource::Index, InnerSource::Hash, Input>) -> Fut + Send + Sync + Clone,
	> ChainSource for AndThen<InnerSource, F>
{
	type Index = InnerSource::Index;
	type Hash = InnerSource::Hash;
	type Data = Fut::Output;

	type Client = AndThenClient<InnerSource::Client, F>;

	async fn stream_and_client(
		&self,
	) -> (BoxChainStream<'_, Self::Index, Self::Hash, Self::Data>, Self::Client) {
		let (inner_stream, inner_client) = self.inner_source.stream_and_client().await;

		#[allow(clippy::redundant_async_block)]
		(
			inner_stream
				.then(move |header| {
					header.and_then_data(move |header| async move { (self.f)(header).await })
				})
				.into_box(),
			AndThenClient::new(inner_client, self.f.clone()),
		)
	}
}

impl<
		Input: aliases::Data,
		Output: aliases::Data,
		Error: aliases::Data,
		InnerSource: ExternalChainSource<Data = Result<Input, Error>>,
		Fut: Future<Output = Result<Output, Error>> + Send,
		F: Fn(Header<InnerSource::Index, InnerSource::Hash, Input>) -> Fut + Send + Sync + Clone,
	> ExternalChainSource for AndThen<InnerSource, F>
{
	type Chain = InnerSource::Chain;
}

#[derive(Clone)]
pub struct AndThenClient<InnerClient, F> {
	inner_client: InnerClient,
	f: F,
}

impl<InnerClient, F> AndThenClient<InnerClient, F> {
	pub fn new(inner_client: InnerClient, f: F) -> Self {
		Self { inner_client, f }
	}
}

#[async_trait::async_trait]
impl<
		Input: aliases::Data,
		Output: aliases::Data,
		Error: aliases::Data,
		InnerClient: ChainClient<Data = Result<Input, Error>>,
		Fut: Future<Output = Result<Output, Error>> + Send,
		F: Fn(Header<InnerClient::Index, InnerClient::Hash, Input>) -> Fut + Send + Sync + Clone,
	> ChainClient for AndThenClient<InnerClient, F>
{
	type Index = InnerClient::Index;
	type Hash = InnerClient::Hash;
	type Data = Fut::Output;

	async fn header_at_index(
		&self,
		index: Self::Index,
	) -> Header<Self::Index, Self::Hash, Self::Data> {
		self.inner_client.header_at_index(index).await.and_then_data(&self.f).await
	}
}
