use futures::{Future, StreamExt};

/// A wrapper around `futures::stream::FuturesUnordered` that waits instead of returning None when
/// there are no futures to poll.
#[pin_project::pin_project]
pub struct FuturesUnorderedWait<Fut> {
	#[pin]
	futures: futures::stream::FuturesUnordered<Fut>,
}

impl<Fut> FuturesUnorderedWait<Fut> {
	pub fn new() -> Self {
		Self { futures: futures::stream::FuturesUnordered::new() }
	}

	pub fn push(&mut self, future: Fut) {
		self.futures.push(future);
	}

	#[allow(dead_code)]
	async fn next(&mut self) -> Option<Fut::Output>
	where
		Fut: Future,
	{
		if self.futures.is_empty() {
			futures::future::pending().await
		} else {
			self.futures.next().await
		}
	}
}

#[cfg(test)]
mod tests {
	use core::task::Poll;

	use futures::FutureExt;

	use super::*;

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
		let mut stream = FuturesUnorderedWait::<TestFuture>::new();

		assert_eq!(stream.next().now_or_never(), None);
	}

	#[tokio::test]
	async fn many_futures_are_ready() {
		let mut stream = FuturesUnorderedWait::<TestFuture>::new();

		const READY_FUTURES: u32 = 4;

		for _ in 0..READY_FUTURES {
			stream.push(TestFuture { ready: true });
		}

		for _ in 0..READY_FUTURES {
			assert_eq!(stream.next().now_or_never(), Some(Some(())));
		}

		assert_eq!(stream.next().now_or_never(), None);
	}

	#[tokio::test]
	async fn wait_until_ready() {
		let mut stream = FuturesUnorderedWait::<TestFuture>::new();
		assert_eq!(stream.next().now_or_never(), None);

		stream.push(TestFuture { ready: true });
		assert_eq!(stream.next().now_or_never(), Some(Some(())));

		assert_eq!(stream.next().now_or_never(), None);

		stream.push(TestFuture { ready: false });
		assert_eq!(stream.next().now_or_never(), None);
	}
}
