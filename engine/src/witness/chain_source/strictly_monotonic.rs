use std::task::Poll;

use futures::Stream;

use super::{BoxChainStream, ChainSource, ChainStream};

#[pin_project::pin_project]
pub struct StrictlyMonotonicStream<Inner: ChainStream> {
	#[pin]
	inner: Inner,
	last_output: Option<Inner::Index>,
}
impl<Inner: ChainStream> Stream for StrictlyMonotonicStream<Inner> {
	type Item = Inner::Item;

	fn poll_next(
		self: std::pin::Pin<&mut Self>,
		cx: &mut std::task::Context<'_>,
	) -> Poll<Option<Self::Item>> {
		let this = self.project();
		match this.inner.poll_next(cx) {
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

pub struct StrictlyMonotonic<Inner: ChainSource> {
	inner: Inner,
}
impl<Inner: ChainSource> StrictlyMonotonic<Inner> {
	pub fn new(inner: Inner) -> Self {
		Self { inner }
	}
}
#[async_trait::async_trait]
impl<Inner: ChainSource> ChainSource for StrictlyMonotonic<Inner> {
	type Index = Inner::Index;
	type Hash = Inner::Hash;
	type Data = Inner::Data;

	type Client = Inner::Client;

	async fn stream_and_client(
		&self,
	) -> (BoxChainStream<'_, Self::Index, Self::Hash, Self::Data>, Self::Client) {
		let (chain_stream, chain_client) = self.inner.stream_and_client().await;
		(Box::pin(StrictlyMonotonicStream { inner: chain_stream, last_output: None }), chain_client)
	}
}
