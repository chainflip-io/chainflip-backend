use futures::TryStream;

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
