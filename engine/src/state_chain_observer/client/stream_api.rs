use futures::Stream;
use utilities::cached_stream::CachedStream;

use super::BlockInfo;

pub const FINALIZED: bool = true;
pub const UNFINALIZED: bool = false;

pub trait StreamApi<const IS_FINALIZED: bool>:
	CachedStream<Item = BlockInfo> + Send + Sync + Unpin + 'static
{
}

#[derive(Clone)]
#[pin_project::pin_project]
pub struct StateChainStream<const IS_FINALIZED: bool, S>(#[pin] S);

impl<const IS_FINALIZED: bool, S: CachedStream> StateChainStream<IS_FINALIZED, S> {
	pub fn new(inner: S) -> Self {
		Self(inner)
	}
}

impl<const IS_FINALIZED: bool, S: Stream> Stream for StateChainStream<IS_FINALIZED, S> {
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
impl<const IS_FINALIZED: bool, S> CachedStream for StateChainStream<IS_FINALIZED, S>
where
	S: CachedStream,
{
	fn cache(&self) -> &Self::Item {
		self.0.cache()
	}
}
impl<
		const IS_FINALIZED: bool,
		S: CachedStream<Item = BlockInfo> + Unpin + Send + Sync + 'static,
	> StreamApi<IS_FINALIZED> for StateChainStream<IS_FINALIZED, S>
{
}
