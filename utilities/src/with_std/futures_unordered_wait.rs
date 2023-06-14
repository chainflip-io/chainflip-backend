use futures::{Future, StreamExt};

/// A wrapper around `futures::stream::FuturesUnordered` that waits instead of returning None when
/// there are no futures to poll.

#[derive(Default)]
pub struct FuturesUnorderedWait<Fut> {
	futures: futures::stream::FuturesUnordered<Fut>,
	count: u32,
}

impl<Fut> FuturesUnorderedWait<Fut> {
	pub fn new() -> Self {
		Self { futures: futures::stream::FuturesUnordered::new(), count: 0 }
	}

	pub fn push(&mut self, future: Fut) {
		self.futures.push(future);
		self.count += 1;
	}

	pub fn count(&self) -> u32 {
		self.count
	}

	pub async fn next(&mut self) -> Option<Fut::Output>
	where
		Fut: Future,
	{
		if self.futures.is_empty() {
			futures::future::pending().await
		} else {
			let res = self.futures.next().await;
			self.count -= 1;
			res
		}
	}
}

#[cfg(test)]
mod tests {
	use core::task::Poll;

	use futures::FutureExt;

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
		let mut stream = FuturesUnorderedWait::<TestFuture>::default();

		assert_eq!(stream.next().now_or_never(), None);
	}

	#[tokio::test]
	async fn one_future_is_ready() {
		let mut stream = FuturesUnorderedWait::<TestFuture>::default();
		stream.push(TestFuture { ready: true });
		assert!(stream.count() == 1);
		assert_eq!(stream.next().now_or_never(), Some(Some(())));

		assert_eq!(stream.next().now_or_never(), None);
	}

	#[tokio::test]
	async fn many_futures_are_ready() {
		let mut stream = FuturesUnorderedWait::<TestFuture>::default();

		const READY_FUTURES: u32 = 4;

		for _ in 0..READY_FUTURES {
			stream.push(TestFuture { ready: true });
		}
		assert_eq!(stream.count(), READY_FUTURES);

		for _ in 0..READY_FUTURES {
			assert_eq!(stream.next().now_or_never(), Some(Some(())));
		}

		assert_eq!(stream.count(), 0);
		assert_eq!(stream.next().now_or_never(), None);
	}

	#[tokio::test]
	async fn wait_until_ready() {
		let mut stream = FuturesUnorderedWait::<TestFuture>::default();
		assert_eq!(stream.next().now_or_never(), None);

		stream.push(TestFuture { ready: true });
		assert_eq!(stream.next().now_or_never(), Some(Some(())));

		assert_eq!(stream.next().now_or_never(), None);

		stream.push(TestFuture { ready: false });
		assert_eq!(stream.next().now_or_never(), None);
	}
}
