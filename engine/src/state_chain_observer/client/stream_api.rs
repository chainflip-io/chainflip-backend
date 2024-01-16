use futures::Stream;
use utilities::CachedStream;

use super::BlockInfo;

pub trait StreamApi<const FINALIZED: bool = true>:
	CachedStream<Item = BlockInfo> + Send + Sync + Unpin + 'static
{
}

#[derive(Clone)]
#[pin_project::pin_project]
pub struct StateChainStream<const FINALIZED: bool, S>(#[pin] S);

impl<const FINALIZED: bool, S: CachedStream> StateChainStream<FINALIZED, S> {
	pub fn new(inner: S) -> Self {
		Self(inner)
	}
}

impl<const FINALIZED: bool, S: Stream> Stream for StateChainStream<FINALIZED, S> {
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
impl<const FINALIZED: bool, S> CachedStream for StateChainStream<FINALIZED, S>
where
	S: CachedStream,
{
	fn cache(&self) -> &Self::Item {
		self.0.cache()
	}
}
impl<const FINALIZED: bool, S: CachedStream<Item = BlockInfo> + Unpin + Send + Sync + 'static>
	StreamApi<FINALIZED> for StateChainStream<FINALIZED, S>
{
}
