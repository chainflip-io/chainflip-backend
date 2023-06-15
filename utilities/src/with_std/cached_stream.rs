use futures::Stream;

pub trait CachedStream: Stream {
	type Cache;

	fn cache(&self) -> &Self::Cache;
}

/// Caches the last mapped last item of a stream according to some map function `f`.
#[derive(Clone)]
#[pin_project::pin_project]
pub struct InnerCachedStream<Stream, Cache, F> {
	#[pin]
	stream: Stream,
	cache: Cache,
	f: F,
}
impl<St, Cache, F> Stream for InnerCachedStream<St, Cache, F>
where
	St: Stream,
	F: FnMut(&St::Item) -> Cache,
{
	type Item = St::Item;

	fn poll_next(
		self: core::pin::Pin<&mut Self>,
		cx: &mut core::task::Context<'_>,
	) -> core::task::Poll<Option<Self::Item>> {
		let this = self.project();
		let poll = this.stream.poll_next(cx);

		if let core::task::Poll::Ready(Some(item)) = &poll {
			*this.cache = (this.f)(item);
		}

		poll
	}

	fn size_hint(&self) -> (usize, Option<usize>) {
		self.stream.size_hint()
	}
}
impl<St, Cache, F> CachedStream for InnerCachedStream<St, Cache, F>
where
	St: Stream,
	F: FnMut(&St::Item) -> Cache,
{
	type Cache = Cache;

	fn cache(&self) -> &Self::Cache {
		&self.cache
	}
}
pub trait MakeCachedStream: Stream {
	fn make_cached<Cache, F: FnMut(&<Self as Stream>::Item) -> Cache>(
		self,
		initial: Cache,
		f: F,
	) -> InnerCachedStream<Self, Cache, F>
	where
		Self: Sized;
}
impl<T: Stream> MakeCachedStream for T {
	fn make_cached<Cache, F: FnMut(&<Self as Stream>::Item) -> Cache>(
		self,
		initial: Cache,
		f: F,
	) -> InnerCachedStream<Self, Cache, F>
	where
		Self: Sized,
	{
		InnerCachedStream { stream: self, cache: initial, f }
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
		let cached_stream = test_stream().make_cached(0, |&item| item * 2);
		assert_eq!(cached_stream.size_hint(), (3, Some(3)));
	}

	#[tokio::test]
	async fn next_and_cached() {
		#[derive(Debug, PartialEq)]
		struct Wrappedi32(i32);

		let mut cached_stream =
			test_stream().make_cached(Wrappedi32(0), |&item| Wrappedi32(item * 2));

		assert_eq!(*cached_stream.cache(), Wrappedi32(0));

		let x = cached_stream.next().await;
		assert_eq!(x, Some(1));
		assert_eq!(*cached_stream.cache(), Wrappedi32(2));

		let x = cached_stream.next().await;
		assert_eq!(x, Some(2));
		assert_eq!(*cached_stream.cache(), Wrappedi32(4));

		let x = cached_stream.next().await;
		assert_eq!(x, Some(3));
		assert_eq!(*cached_stream.cache(), Wrappedi32(6));

		// The cache still exists when we get None from the stream.
		let x = cached_stream.next().await;
		assert_eq!(x, None);
		assert_eq!(*cached_stream.cache(), Wrappedi32(6));
	}
}
