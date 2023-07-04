use std::task::Poll;

use futures::Stream;

use super::{BoxChainStream, ChainSourceWithClient, ChainStream};

#[pin_project::pin_project]
pub struct StrictlyMonotonicStream<Underlying: ChainStream> {
	#[pin]
	underlying: Underlying,
	last_output: Option<Underlying::Index>,
}
impl<Underlying: ChainStream> Stream for StrictlyMonotonicStream<Underlying> {
	type Item = Underlying::Item;

	fn poll_next(
		self: std::pin::Pin<&mut Self>,
		cx: &mut std::task::Context<'_>,
	) -> Poll<Option<Self::Item>> {
		let this = self.project();
		match this.underlying.poll_next(cx) {
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

pub struct StrictlyMonotonic<Underlying: ChainSourceWithClient> {
	underlying: Underlying,
}
impl<Underlying: ChainSourceWithClient> StrictlyMonotonic<Underlying> {
	pub fn new(underlying: Underlying) -> Self {
		Self { underlying }
	}
}
#[async_trait::async_trait]
impl<Underlying: ChainSourceWithClient> ChainSourceWithClient for StrictlyMonotonic<Underlying> {
	type Index = Underlying::Index;
	type Hash = Underlying::Hash;
	type Data = Underlying::Data;

	type Client = Underlying::Client;

	async fn stream_and_client(
		&self,
	) -> (BoxChainStream<'_, Self::Index, Self::Hash, Self::Data>, Self::Client) {
		let (chain_stream, chain_client) = self.underlying.stream_and_client().await;
		(
			Box::pin(StrictlyMonotonicStream { underlying: chain_stream, last_output: None }),
			chain_client,
		)
	}
}
