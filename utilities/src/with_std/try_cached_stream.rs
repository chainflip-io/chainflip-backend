use futures::{Stream, TryStream};

pub trait TryCachedStream: Stream<Item = Result<Self::Ok, Self::Error>> {
	type Ok: Clone;
	type Error;

	fn cache(&self) -> &Self::Ok;
}
impl<St> TryCachedStream for Box<St>
where
	St: TryCachedStream + Unpin + ?Sized,
{
	type Ok = St::Ok;
	type Error = St::Error;

	fn cache(&self) -> &Self::Ok {
		(**self).cache()
	}
}
impl<P: core::ops::DerefMut + Unpin> TryCachedStream for std::pin::Pin<P>
where
	<P as core::ops::Deref>::Target: TryCachedStream,
{
	type Ok = <<P as core::ops::Deref>::Target as TryCachedStream>::Ok;
	type Error = <<P as core::ops::Deref>::Target as TryCachedStream>::Error;

	fn cache(&self) -> &Self::Ok {
		(**self).cache()
	}
}

/// Caches the last mapped last item of a stream according to some map function `f`.
#[derive(Clone)]
#[pin_project::pin_project]
pub struct InnerTryCachedStream<St: TryStream> {
	#[pin]
	stream: St,
	cache: St::Ok,
}
impl<St: TryStream> InnerTryCachedStream<St> {
	pub fn into_inner(self) -> St {
		self.stream
	}
}
impl<St> Stream for InnerTryCachedStream<St>
where
	St: TryStream,
	St::Ok: Clone,
{
	type Item = Result<St::Ok, St::Error>;

	fn poll_next(
		self: core::pin::Pin<&mut Self>,
		cx: &mut core::task::Context<'_>,
	) -> core::task::Poll<Option<Self::Item>> {
		let this = self.project();
		let poll = this.stream.try_poll_next(cx);

		if let core::task::Poll::Ready(Some(Ok(item))) = &poll {
			*this.cache = item.clone();
		}

		poll
	}

	fn size_hint(&self) -> (usize, Option<usize>) {
		self.stream.size_hint()
	}
}
impl<St> TryCachedStream for InnerTryCachedStream<St>
where
	St: TryStream,
	St::Ok: Clone,
{
	type Ok = St::Ok;
	type Error = St::Error;

	fn cache(&self) -> &Self::Ok {
		&self.cache
	}
}
pub trait MakeTryCachedStream: TryStream
where
	<Self as TryStream>::Ok: Clone,
{
	fn make_try_cached(self, initial: <Self as TryStream>::Ok) -> InnerTryCachedStream<Self>
	where
		Self: Sized;
}
impl<T: TryStream> MakeTryCachedStream for T
where
	<Self as TryStream>::Ok: Clone,
{
	fn make_try_cached(self, initial: <Self as TryStream>::Ok) -> InnerTryCachedStream<Self>
	where
		Self: Sized,
	{
		InnerTryCachedStream { stream: self, cache: initial }
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
		let cached_stream = test_stream().make_try_cached(0);
		assert_eq!(cached_stream.size_hint(), (3, Some(3)));
	}

	#[tokio::test]
	async fn next_on_empty() {
		let mut cached_stream = stream::empty::<Result<i32, i32>>().make_try_cached(0);
		assert_eq!(cached_stream.next().await, None);
	}

	#[tokio::test]
	async fn next_and_cached() {
		#[derive(Debug, PartialEq)]
		struct Wrappedi32(i32);

		let mut cached_stream = test_stream().make_try_cached(0);

		assert_eq!(*cached_stream.cache(), 0);

		let x = cached_stream.next().await;
		assert_eq!(x, Some(Ok(1)));
		assert_eq!(*cached_stream.cache(), 1);

		let x = cached_stream.next().await;
		assert_eq!(x, Some(Err(2)));
		assert_eq!(*cached_stream.cache(), 1);

		let x = cached_stream.next().await;
		assert_eq!(x, Some(Ok(3)));
		assert_eq!(*cached_stream.cache(), 3);

		// The cache still exists when we get None from the stream.
		let x = cached_stream.next().await;
		assert_eq!(x, None);
		assert_eq!(*cached_stream.cache(), 3);
	}
}
