use futures::Stream;

pub trait CachedStream: Stream {
	fn cache(&self) -> &Self::Item;
}
impl<St> CachedStream for Box<St>
where
	St: CachedStream + Unpin + ?Sized,
{
	fn cache(&self) -> &Self::Item {
		(**self).cache()
	}
}
impl<P: core::ops::DerefMut + Unpin> CachedStream for std::pin::Pin<P>
where
	<P as core::ops::Deref>::Target: CachedStream,
{
	fn cache(&self) -> &Self::Item {
		(**self).cache()
	}
}

/// Caches the last item of a stream according to some map function `f`.
#[derive(Clone)]
#[pin_project::pin_project]
pub struct InnerCachedStream<St: Stream> {
	#[pin]
	stream: St,
	cache: St::Item,
}
impl<St: Stream> InnerCachedStream<St> {
	pub fn into_inner(self) -> St {
		self.stream
	}
}
impl<St> Stream for InnerCachedStream<St>
where
	St: Stream,
	St::Item: Clone,
{
	type Item = St::Item;

	fn poll_next(
		self: core::pin::Pin<&mut Self>,
		cx: &mut core::task::Context<'_>,
	) -> core::task::Poll<Option<Self::Item>> {
		let this = self.project();
		let poll = this.stream.poll_next(cx);

		if let core::task::Poll::Ready(Some(item)) = &poll {
			*this.cache = item.clone();
		}

		poll
	}

	fn size_hint(&self) -> (usize, Option<usize>) {
		self.stream.size_hint()
	}
}
impl<St> CachedStream for InnerCachedStream<St>
where
	St: Stream,
	St::Item: Clone,
{
	fn cache(&self) -> &Self::Item {
		&self.cache
	}
}
pub trait MakeCachedStream: Stream {
	fn make_cached(self, initial: Self::Item) -> InnerCachedStream<Self>
	where
		Self: Sized;
}
impl<T: Stream> MakeCachedStream for T {
	fn make_cached(self, initial: Self::Item) -> InnerCachedStream<Self>
	where
		Self: Sized,
	{
		InnerCachedStream { stream: self, cache: initial }
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use futures::{stream, StreamExt};

	fn test_stream() -> impl Stream<Item = i32> {
		stream::iter(vec![1, 2, 3])
	}

	#[test]
	fn size_hint() {
		let cached_stream = test_stream().make_cached(0);
		assert_eq!(cached_stream.size_hint(), (3, Some(3)));
	}

	#[tokio::test]
	async fn next_on_empty() {
		let mut cached_stream = stream::empty::<i32>().make_cached(0);
		assert_eq!(cached_stream.next().await, None);
	}

	#[tokio::test]
	async fn next_and_cached() {
		#[derive(Debug, PartialEq)]
		struct Wrappedi32(i32);

		let mut cached_stream = test_stream().make_cached(0);

		assert_eq!(*cached_stream.cache(), 0);

		let x = cached_stream.next().await;
		assert_eq!(x, Some(1));
		assert_eq!(*cached_stream.cache(), 1);

		let x = cached_stream.next().await;
		assert_eq!(x, Some(2));
		assert_eq!(*cached_stream.cache(), 2);

		let x = cached_stream.next().await;
		assert_eq!(x, Some(3));
		assert_eq!(*cached_stream.cache(), 3);

		// The cache still exists when we get None from the stream.
		let x = cached_stream.next().await;
		assert_eq!(x, None);
		assert_eq!(*cached_stream.cache(), 3);
	}
}
