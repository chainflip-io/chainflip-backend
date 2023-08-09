use std::task::Poll;

use futures::Stream;

use crate::witness::common::ExternalChainSource;

use super::{BoxChainStream, ChainSource, ChainStream};

#[pin_project::pin_project]
pub struct StrictlyMonotonicStream<InnerStream: ChainStream> {
	#[pin]
	inner_stream: InnerStream,
	last_output: Option<InnerStream::Index>,
}
impl<InnerStream: ChainStream> Stream for StrictlyMonotonicStream<InnerStream> {
	type Item = InnerStream::Item;

	fn poll_next(
		self: std::pin::Pin<&mut Self>,
		cx: &mut std::task::Context<'_>,
	) -> Poll<Option<Self::Item>> {
		let this = self.project();
		match this.inner_stream.poll_next(cx) {
			Poll::Ready(Some(header)) => Poll::Ready(if Some(header.index) > *this.last_output {
				*this.last_output = Some(header.index);
				Some(header)
			} else {
				None
			}),
			poll_next => poll_next,
		}
	}
}

#[derive(Clone)]
pub struct StrictlyMonotonic<InnerSource: ChainSource> {
	inner_source: InnerSource,
}
impl<InnerSource: ChainSource> StrictlyMonotonic<InnerSource> {
	pub fn new(inner_source: InnerSource) -> Self {
		Self { inner_source }
	}
}
#[async_trait::async_trait]
impl<InnerSource: ChainSource> ChainSource for StrictlyMonotonic<InnerSource> {
	type Index = InnerSource::Index;
	type Hash = InnerSource::Hash;
	type Data = InnerSource::Data;

	type Client = InnerSource::Client;

	async fn stream_and_client(
		&self,
	) -> (BoxChainStream<'_, Self::Index, Self::Hash, Self::Data>, Self::Client) {
		let (inner_stream, inner_client) = self.inner_source.stream_and_client().await;
		(Box::pin(StrictlyMonotonicStream { inner_stream, last_output: None }), inner_client)
	}
}

impl<InnerSource: ExternalChainSource> ExternalChainSource for StrictlyMonotonic<InnerSource> {
	type Chain = InnerSource::Chain;
}
