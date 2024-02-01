use futures::{Future, Stream};

#[pin_project::pin_project]
pub struct NextOrPending<'a, St: ?Sized> {
	#[pin]
	stream: &'a mut St,
}
impl<'a, St: Stream + ?Sized + Unpin> Future for NextOrPending<'a, St> {
	type Output = <St as Stream>::Item;

	fn poll(
		self: core::pin::Pin<&mut Self>,
		cx: &mut core::task::Context<'_>,
	) -> core::task::Poll<Self::Output> {
		let this = self.project();
		match this.stream.poll_next(cx) {
			core::task::Poll::Ready(Some(item)) => core::task::Poll::Ready(item),
			_ => core::task::Poll::Pending,
		}
	}
}

pub trait UnendingStream: Stream {
	/// Returns the next item in the stream. If the stream is empty it will return `Pending` instead
	/// of returnin `None`. This is useful in scenarios where a stream might be empty for a while
	/// before a new item is added, such as when using a `FuturesUnordered`.
	fn next_or_pending(&mut self) -> NextOrPending<'_, Self>
	where
		Self: Unpin,
	{
		NextOrPending { stream: self }
	}
}
impl<T: Stream + ?Sized + Unpin> UnendingStream for T {}

#[cfg(test)]
mod tests {
	use core::task::Poll;

	use futures::{stream::FuturesUnordered, FutureExt};

	use super::*;

	#[derive(Default)]
	pub struct TestFuture {
		ready: bool,
	}

	impl Future for TestFuture {
		type Output = ();

		fn poll(
			self: core::pin::Pin<&mut Self>,
			_: &mut core::task::Context<'_>,
		) -> Poll<Self::Output> {
			let this = self.get_mut();

			if this.ready {
				Poll::Ready(())
			} else {
				Poll::Pending
			}
		}
	}

	#[tokio::test]
	async fn no_futures_stays_pending() {
		let mut stream = FuturesUnordered::<TestFuture>::default();

		assert_eq!(stream.next_or_pending().now_or_never(), None);
	}

	#[tokio::test]
	async fn one_future_is_ready() {
		let mut stream = FuturesUnordered::<TestFuture>::default();
		stream.push(TestFuture { ready: true });

		assert_eq!(stream.next_or_pending().now_or_never(), Some(()));

		assert_eq!(stream.next_or_pending().now_or_never(), None);
	}

	#[tokio::test]
	async fn many_futures_are_ready() {
		let mut stream = FuturesUnordered::<TestFuture>::default();

		const READY_FUTURES: u32 = 4;

		for _ in 0..READY_FUTURES {
			stream.push(TestFuture { ready: true });
		}

		for _ in 0..READY_FUTURES {
			assert_eq!(stream.next_or_pending().now_or_never(), Some(()));
		}

		assert_eq!(stream.next_or_pending().now_or_never(), None);
	}

	#[tokio::test]
	async fn wait_until_ready() {
		let mut stream = FuturesUnordered::<TestFuture>::default();
		assert_eq!(stream.next_or_pending().now_or_never(), None);

		stream.push(TestFuture { ready: true });
		assert_eq!(stream.next_or_pending().now_or_never(), Some(()));

		assert_eq!(stream.next_or_pending().now_or_never(), None);

		stream.push(TestFuture { ready: false });
		assert_eq!(stream.next_or_pending().now_or_never(), None);
	}
}
