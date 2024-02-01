use futures_util::StreamExt;

use crate::witness::common::ExternalChainSource;

use super::{BoxChainStream, ChainSource};

#[derive(Clone)]
pub struct Logging<InnerSource: ChainSource> {
	inner_source: InnerSource,
	log_prefix: &'static str,
}
impl<InnerSource: ChainSource> Logging<InnerSource> {
	pub fn new(inner_source: InnerSource, log_prefix: &'static str) -> Self {
		Self { inner_source, log_prefix }
	}
}

#[async_trait::async_trait]
impl<InnerSource: ChainSource + ExternalChainSource> ChainSource for Logging<InnerSource>
where
	InnerSource::Client: Clone,
{
	type Index = InnerSource::Index;
	type Hash = InnerSource::Hash;
	type Data = InnerSource::Data;

	type Client = InnerSource::Client;

	async fn stream_and_client(
		&self,
	) -> (BoxChainStream<'_, Self::Index, Self::Hash, Self::Data>, Self::Client) {
		let (chain_stream, chain_client) = self.inner_source.stream_and_client().await;
		(
			Box::pin(chain_stream.then(move |header| async move {
				tracing::info!(
					"{} | {}: index: {:?} hash: {:?}",
					<<InnerSource as ExternalChainSource>::Chain as cf_chains::Chain>::NAME,
					self.log_prefix,
					header.index,
					header.hash,
				);
				header
			})),
			chain_client,
		)
	}
}

impl<InnerSource: ExternalChainSource> ExternalChainSource for Logging<InnerSource>
where
	InnerSource::Client: Clone,
{
	type Chain = InnerSource::Chain;
}
