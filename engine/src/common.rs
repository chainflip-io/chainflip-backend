use std::{
	fmt::Display,
	ops::{Deref, DerefMut},
	path::Path,
	time::Duration,
};

use anyhow::Context;
use futures::{Future, Stream, TryStream};
use itertools::Itertools;

struct MutexStateAndPoisonFlag<T> {
	poisoned: bool,
	state: T,
}

pub struct MutexGuard<'a, T> {
	guard: tokio::sync::MutexGuard<'a, MutexStateAndPoisonFlag<T>>,
}
impl<'a, T> Deref for MutexGuard<'a, T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		&self.guard.deref().state
	}
}
impl<'a, T> DerefMut for MutexGuard<'a, T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.guard.deref_mut().state
	}
}
impl<'a, T> Drop for MutexGuard<'a, T> {
	fn drop(&mut self) {
		let guarded = self.guard.deref_mut();
		if !guarded.poisoned && std::thread::panicking() {
			guarded.poisoned = true;
		}
	}
}

/// This mutex implementation will panic when it is locked iff a thread previously panicked while
/// holding it. This ensures potentially broken data cannot be seen by other threads.
pub struct Mutex<T> {
	mutex: tokio::sync::Mutex<MutexStateAndPoisonFlag<T>>,
}
impl<T> Mutex<T> {
	pub fn new(t: T) -> Self {
		Self {
			mutex: tokio::sync::Mutex::new(MutexStateAndPoisonFlag { poisoned: false, state: t }),
		}
	}
	pub async fn lock(&self) -> MutexGuard<'_, T> {
		let guard = self.mutex.lock().await;

		if guard.deref().poisoned {
			panic!("Another thread panicked while holding this lock");
		} else {
			MutexGuard { guard }
		}
	}
}

#[cfg(test)]
mod tests {
	use utilities::assert_future_panics;

	use super::*;
	use std::sync::Arc;

	#[tokio::test]
	async fn mutex_panics_if_poisoned() {
		let mutex = Arc::new(Mutex::new(0));
		{
			let mutex_clone = mutex.clone();
			tokio::spawn(async move {
				let _inner = mutex_clone.lock().await;
				panic!();
			})
			.await
			.unwrap_err();
		}
		assert_future_panics!(mutex.lock());
	}

	#[tokio::test]
	async fn mutex_doesnt_panic_if_not_poisoned() {
		let mutex = Arc::new(Mutex::new(0));
		{
			let mutex_clone = mutex.clone();
			tokio::spawn(async move {
				let _inner = mutex_clone.lock().await;
			})
			.await
			.unwrap();
		}
		mutex.lock().await;
	}
}

/// Starts a task and restarts if it fails.
/// If it succeeds it will terminate, and not attempt a restart.
/// The `StaticState` is used to allow for state to be shared between restarts.
/// Such as a Receiver a task might need to continue to receive data from some other task,
/// despite the fact it has been restarted.
pub async fn start_with_restart_on_failure<TaskFut, TaskGenerator>(task_generator: TaskGenerator)
where
	TaskFut: Future<Output = Result<(), ()>> + Send + 'static,
	TaskGenerator: Fn() -> TaskFut,
{
	// Spawn with handle and then wait for future to finish
	while task_generator().await.is_err() {
		// give it some time before the restart
		tokio::time::sleep(Duration::from_secs(2)).await;
	}
}

#[cfg(test)]
mod test_restart_on_failure {

	use super::*;

	#[tokio::test(start_paused = true)]
	async fn test_restart_on_failure() {
		use std::sync::{Arc, Mutex};
		let restart_count = Arc::new(Mutex::new(0));
		let restart_count_to_move = restart_count.clone();

		const TARGET: usize = 6;

		let start_up_some_loop = move || {
			let restart_count = restart_count_to_move.clone();
			async move {
				let mut restart_count = restart_count.lock().unwrap();
				*restart_count += 1;

				if *restart_count == TARGET {
					return Ok(())
				}

				for i in 0..10 {
					if i == 4 {
						return Err(())
					}
				}

				panic!("Should not reach here");
			}
		};

		start_with_restart_on_failure(start_up_some_loop).await;

		assert_eq!(*restart_count.lock().unwrap(), TARGET);
	}
}

pub fn read_clean_and_decode_hex_str_file<V, T: FnOnce(&str) -> Result<V, anyhow::Error>>(
	file: &Path,
	context: &str,
	t: T,
) -> Result<V, anyhow::Error> {
	std::fs::read_to_string(file)
		.map_err(anyhow::Error::new)
		.with_context(|| format!("Failed to read {} file at {}", context, file.display()))
		.and_then(|string| {
			let mut str = string.as_str();
			str = str.trim();
			str = str.trim_matches(['"', '\''].as_ref());
			if let Some(stripped_str) = str.strip_prefix("0x") {
				str = stripped_str;
			}
			// Note if str is valid hex or not is determined by t()
			t(str)
		})
		.with_context(|| format!("Failed to decode {} file at {}", context, file.display()))
}

#[cfg(test)]
mod tests_read_clean_and_decode_hex_str_file {
	use crate::testing::with_file;
	use utilities::assert_ok;

	use super::*;

	#[test]
	fn load_hex_file() {
		with_file(b"   \"\'\'\"0xhex\"\'  ", |file_path| {
			assert_eq!(
				assert_ok!(read_clean_and_decode_hex_str_file(file_path, "TEST", |str| Ok(
					str.to_string()
				))),
				"hex".to_string()
			);
		});
	}

	#[test]
	fn load_invalid_hex_file() {
		with_file(b"   h\" \'ex  ", |file_path| {
			assert_eq!(
				assert_ok!(read_clean_and_decode_hex_str_file(file_path, "TEST", |str| Ok(
					str.to_string()
				))),
				"h\" \'ex".to_string()
			);
		});
	}
}

pub fn format_iterator<'a, It: 'a + IntoIterator>(it: It) -> itertools::Format<'a, It::IntoIter>
where
	It::Item: Display,
{
	it.into_iter().format(", ")
}

pub fn all_same<Item: PartialEq, It: IntoIterator<Item = Item>>(it: It) -> Option<Item> {
	let mut it = it.into_iter();
	let option_item = it.next();
	match option_item {
		Some(item) =>
			if it.all(|other_items| other_items == item) {
				Some(item)
			} else {
				None
			},
		None => panic!(),
	}
}

pub fn split_at<C: FromIterator<It::Item>, It: IntoIterator>(it: It, index: usize) -> (C, C)
where
	It::IntoIter: ExactSizeIterator,
{
	struct IteratorRef<'a, T, It: Iterator<Item = T>> {
		it: &'a mut It,
	}
	impl<'a, T, It: Iterator<Item = T>> Iterator for IteratorRef<'a, T, It> {
		type Item = T;

		fn next(&mut self) -> Option<Self::Item> {
			self.it.next()
		}
	}

	let mut it = it.into_iter();
	assert!(index < it.len());
	let wrapped_it = IteratorRef { it: &mut it };
	(wrapped_it.take(index).collect(), it.collect())
}

#[test]
fn test_split_at() {
	let (left, right) = split_at::<Vec<_>, _>(vec![4, 5, 6, 3, 4, 5], 3);

	assert_eq!(&left[..], &[4, 5, 6]);
	assert_eq!(&right[..], &[3, 4, 5]);
}

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

		use crate::common::EngineTryStreamExt;

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
