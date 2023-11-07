use futures::{Stream, TryStream};

pub trait TryCachedStream: Stream {
	type Cache;

	fn cache(&self) -> &Self::Cache;
}

/// Caches the last mapped last item of a stream according to some map function `f`.
#[derive(Clone)]
#[pin_project::pin_project]
pub struct InnerTryCachedStream<Stream, Cache, F> {
	#[pin]
	stream: Stream,
	cache: Cache,
	f: F,
}
impl<St, Cache, F> InnerTryCachedStream<St, Cache, F> {
	pub fn into_inner(self) -> St {
		self.stream
	}
}
impl<St, Cache, F> Stream for InnerTryCachedStream<St, Cache, F>
where
	St: TryStream,
	F: FnMut(&St::Ok) -> Cache,
{
	type Item = Result<St::Ok, St::Error>;

	fn poll_next(
		self: core::pin::Pin<&mut Self>,
		cx: &mut core::task::Context<'_>,
	) -> core::task::Poll<Option<Self::Item>> {
		let this = self.project();
		let poll = this.stream.try_poll_next(cx);

		if let core::task::Poll::Ready(Some(Ok(item))) = &poll {
			*this.cache = (this.f)(item);
		}

		poll
	}

	fn size_hint(&self) -> (usize, Option<usize>) {
		self.stream.size_hint()
	}
}
impl<St, Cache, F> TryCachedStream for InnerTryCachedStream<St, Cache, F>
where
	St: TryStream,
	F: FnMut(&St::Ok) -> Cache,
{
	type Cache = Cache;

	fn cache(&self) -> &Self::Cache {
		&self.cache
	}
}
pub trait MakeTryCachedStream: TryStream {
	fn make_try_cached<Cache, F: FnMut(&<Self as TryStream>::Ok) -> Cache>(
		self,
		initial: Cache,
		f: F,
	) -> InnerTryCachedStream<Self, Cache, F>
	where
		Self: Sized;
}
impl<T: TryStream> MakeTryCachedStream for T {
	fn make_try_cached<Cache, F: FnMut(&<Self as TryStream>::Ok) -> Cache>(
		self,
		initial: Cache,
		f: F,
	) -> InnerTryCachedStream<Self, Cache, F>
	where
		Self: Sized,
	{
		InnerTryCachedStream { stream: self, cache: initial, f }
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use futures::{stream, StreamExt};

	fn test_stream() -> impl TryStream<Ok = i32, Error = i32> {
		stream::iter(vec![Ok(1), Err(2), Ok(3)])
	}

	#[test]
	fn size_hint() {
		let cached_stream = test_stream().make_try_cached(0, |&item| item * 2);
		assert_eq!(cached_stream.size_hint(), (3, Some(3)));
	}

	#[tokio::test]
	async fn next_on_empty() {
		let mut cached_stream =
			stream::empty::<Result<i32, i32>>().make_try_cached(0, |&item| item * 2);
		assert_eq!(cached_stream.next().await, None);
	}

	#[tokio::test]
	async fn next_and_cached() {
		#[derive(Debug, PartialEq)]
		struct Wrappedi32(i32);

		let mut cached_stream =
			test_stream().make_try_cached(Wrappedi32(0), |&item| Wrappedi32(item * 2));

		assert_eq!(*cached_stream.cache(), Wrappedi32(0));

		let x = cached_stream.next().await;
		assert_eq!(x, Some(Ok(1)));
		assert_eq!(*cached_stream.cache(), Wrappedi32(2));

		let x = cached_stream.next().await;
		assert_eq!(x, Some(Err(2)));
		assert_eq!(*cached_stream.cache(), Wrappedi32(2));

		let x = cached_stream.next().await;
		assert_eq!(x, Some(Ok(3)));
		assert_eq!(*cached_stream.cache(), Wrappedi32(6));

		// The cache still exists when we get None from the stream.
		let x = cached_stream.next().await;
		assert_eq!(x, None);
		assert_eq!(*cached_stream.cache(), Wrappedi32(6));
	}
}
