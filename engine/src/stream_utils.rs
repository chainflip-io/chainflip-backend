use std::time::Duration;

use futures::{Stream, TryStream};

pub trait EngineTryStreamExt: TryStream + Sized {
	fn end_after_error(self) -> end_after_error::EndAfterError<Self> {
		end_after_error::EndAfterError::new(self)
	}
}

impl<St: TryStream + Sized> EngineTryStreamExt for St {}

mod end_after_error {

	use futures::{Stream, TryStream};
	use futures_core::FusedStream;
	use pin_project::pin_project;
	use std::{fmt, pin::Pin, task::Poll};

	/// Stream for the [`end_after_error`](super::EngineTryStreamExt::end_after_error) method.
	#[must_use = "streams do nothing unless polled"]
	#[pin_project]
	pub struct EndAfterError<St: TryStream> {
		#[pin]
		stream: St,
		done_taking: bool,
	}

	impl<St> fmt::Debug for EndAfterError<St>
	where
		St: TryStream + fmt::Debug,
	{
		fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
			f.debug_struct("EndAfterError")
				.field("stream", &self.stream)
				.field("done_taking", &self.done_taking)
				.finish()
		}
	}

	impl<St> EndAfterError<St>
	where
		St: TryStream,
	{
		pub(super) fn new(stream: St) -> Self {
			Self { stream, done_taking: false }
		}
	}

	impl<St> Stream for EndAfterError<St>
	where
		St: TryStream,
	{
		type Item = Result<St::Ok, St::Error>;

		fn poll_next(
			self: Pin<&mut Self>,
			cx: &mut std::task::Context<'_>,
		) -> Poll<Option<Self::Item>> {
			let mut this = self.project();

			Poll::Ready(if *this.done_taking {
				None
			} else if let Some(result) = futures::ready!(this.stream.as_mut().try_poll_next(cx)) {
				if result.is_err() {
					*this.done_taking = true;
				}
				Some(result)
			} else {
				*this.done_taking = true;
				None
			})
		}

		fn size_hint(&self) -> (usize, Option<usize>) {
			if self.done_taking {
				(0, Some(0))
			} else {
				let (lower, upper) = self.stream.size_hint();
				(std::cmp::min(1, lower), upper) // lower bound can always be 1 if the next result.is_err()
			}
		}
	}

	impl<St> FusedStream for EndAfterError<St>
	where
		St: TryStream + FusedStream,
	{
		fn is_terminated(&self) -> bool {
			self.done_taking || self.stream.is_terminated()
		}
	}

	#[cfg(test)]
	mod tests {
		use futures::StreamExt;

		use crate::stream_utils::EngineTryStreamExt;

		#[tokio::test]
		async fn end_after_error_from_underlying_stream() {
			assert_eq!(
				&[Ok(1), Err(2)],
				&futures::stream::iter([Ok(1), Err(2), Ok(3)])
					.end_after_error()
					.collect::<Vec<_>>()
					.await[..]
			);
		}

		#[tokio::test]
		async fn end_when_underlying_stream_ends() {
			let underlying_stream = [Ok::<u32, u32>(1), Ok(2), Ok(3)];
			assert_eq!(
				&underlying_stream,
				&futures::stream::iter(underlying_stream)
					.end_after_error()
					.collect::<Vec<_>>()
					.await[..]
			);
		}
	}
}

pub trait EngineStreamExt: Stream + Sized {
	fn timeout_after(self, duration: Duration) -> timeout_stream::TimeoutStream<Self> {
		timeout_stream::TimeoutStream::new(self, duration)
	}
}

impl<St: Stream + Sized> EngineStreamExt for St {}

mod timeout_stream {
	use std::{pin::Pin, task::Poll, time::Duration};

	use futures::{FutureExt, Stream, StreamExt};
	use pin_project::pin_project;
	use tokio::time::timeout;

	/// Stream for the [`timeout_after`](super::EngineStreamExt::timeout_after) method.
	#[must_use = "streams do nothing unless polled"]
	#[pin_project]
	pub struct TimeoutStream<S> {
		#[pin]
		stream: S,
		timeout: Duration,
	}

	impl<S> TimeoutStream<S> {
		pub fn new(stream: S, timeout: Duration) -> Self {
			Self { stream, timeout }
		}
	}

	impl<S> Stream for TimeoutStream<S>
	where
		S: StreamExt + Unpin,
	{
		type Item = Result<S::Item, anyhow::Error>;

		fn poll_next(
			mut self: Pin<&mut Self>,
			cx: &mut std::task::Context<'_>,
		) -> Poll<Option<Self::Item>> {
			let mut fut = Box::pin(timeout(self.timeout, self.stream.next()));
			fut.poll_unpin(cx)
				.map(|res| res.transpose())
				.map_err(|e| anyhow::anyhow!("Stream timed out waiting for item: {e}"))
		}
	}

	#[cfg(test)]
	mod tests {

		use futures::stream;

		use super::*;

		#[tokio::test]
		async fn stream_returns_none_on_no_items() {
			let mut stream =
				TimeoutStream::new(futures::stream::empty::<u64>(), Duration::from_secs(1));
			assert!(stream.next().await.is_none());
		}

		#[tokio::test]
		async fn test_timeout_stream_ok() {
			// There is no delay here, so we should get all the items.
			let stream = stream::iter([1, 2, 3]);

			let mut timeout_stream = TimeoutStream::new(stream, Duration::from_millis(50));

			assert_eq!(timeout_stream.next().await.unwrap().unwrap(), 1);
			assert_eq!(timeout_stream.next().await.unwrap().unwrap(), 2);
			assert_eq!(timeout_stream.next().await.unwrap().unwrap(), 3);
			assert!(timeout_stream.next().await.is_none());
		}

		#[tokio::test(start_paused = true)]
		async fn test_timeout_stream_timeout() {
			const DELAY_DURATION_MILLIS: u64 = 100;

			let delayed_stream = |items: Vec<i32>| {
				let items = items.into_iter();
				Box::pin(stream::unfold(items, |mut items| async move {
					if let Some(i) = items.next() {
						tokio::time::sleep(Duration::from_millis(DELAY_DURATION_MILLIS)).await;
						Some((i, items))
					} else {
						None
					}
				}))
			};

			// We should get the items from this one, since the timeout is double the delay.
			let mut timeout_stream = TimeoutStream::new(
				delayed_stream(vec![1, 2, 3]),
				Duration::from_millis(DELAY_DURATION_MILLIS + DELAY_DURATION_MILLIS),
			);

			assert_eq!(timeout_stream.next().await.unwrap().unwrap(), 1);
			assert_eq!(timeout_stream.next().await.unwrap().unwrap(), 2);
			assert_eq!(timeout_stream.next().await.unwrap().unwrap(), 3);
			assert!(timeout_stream.next().await.is_none());

			// We should get a timeout error from this one, since the timeout is less than the
			// delay.
			let mut timeout_stream =
				TimeoutStream::new(delayed_stream(vec![1, 2, 3]), Duration::default());

			assert!(timeout_stream.next().await.unwrap().is_err());
			assert!(timeout_stream.next().await.unwrap().is_err());
			assert!(timeout_stream.next().await.unwrap().is_err());
			// We haven't actually pulled an item off the stream, and we never will, so we should
			// get errors indefinitely.
			assert!(timeout_stream.next().await.unwrap().is_err());
		}
	}
}
