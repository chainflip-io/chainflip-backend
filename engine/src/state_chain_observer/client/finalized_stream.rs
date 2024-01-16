use futures::Stream;
use utilities::CachedStream;

#[derive(Clone)]
#[pin_project::pin_project]
pub struct FinalizedCachedStream<S>(#[pin] S);

impl<S: Stream + CachedStream> FinalizedCachedStream<S> {
	pub fn new(inner: S) -> Self {
		FinalizedCachedStream(inner)
	}
}

impl<S: Stream + CachedStream> Stream for FinalizedCachedStream<S> {
	type Item = <S as Stream>::Item;

	fn poll_next(
		self: core::pin::Pin<&mut Self>,
		cx: &mut core::task::Context<'_>,
	) -> core::task::Poll<Option<Self::Item>> {
		self.project().0.poll_next(cx)
	}

	fn size_hint(&self) -> (usize, Option<usize>) {
		self.0.size_hint()
	}
}

impl<S> CachedStream for FinalizedCachedStream<S>
where
	S: Stream + CachedStream,
{
	fn cache(&self) -> &Self::Item {
		self.0.cache()
	}
}
