//! # Task Scope
//!
//! The idea here is very similiar to the [thread_scope](https://doc.rust-lang.org/1.63.0/std/thread/fn.scope.html) feature in the std library.
//! The important differences being it:
//!     - is designed to work with futures
//!     - propagates errors returned by tasks ([instead of only panics](https://doc.rust-lang.org/1.63.0/std/thread/fn.scope.html#panics))
//!     - when tasks panic or return errors, this will cause all other still running tasks to be [cancelled](https://blog.yoshuawuyts.com/async-cancellation-1/)
//!
//! A scope is designed to allow you to spawn asynchronous tasks, wait for all those tasks to
//! finish, and handle errors/panics caused by those tasks.
//!
//! When you create a scope, you must provide a parent task/"async closure", which is passed a
//! handle via which you can spawn further child tasks, which run asychronously to the parent task.
//! The scope will not exit/return until all the tasks have completed. Iff any of the scope's tasks
//! panic or return an error, the scope will cancel all remaining tasks, and end by respectively
//! panicking or returning the error.
//!
//! For the public functions in this module, if they are used unsafely the code will not compile.
//!
//! # Usage
//!
//! `scope.spawn()` should be used instead of `tokio::spawn()`.
//!
//! `scope.spawn_blocking()` (To be added) should be used instead of `tokio::spawn_blocking()`
//! unless you are running an operation that is guaranteed to exit after a finite period and you are
//! awaiting on the JoinHandle immediately after spawning. This exception is made as in this case
//! the task_scope system offers no advantage, and may make the code more complex as you need to
//! pass around a scope. TODO: Possibly introduce a function to express this exception.
//!
//! Where `scope.spawn_blocking()` is used for long running operations the developer must ensure
//! that if the rest of the non-spawn-blocking tasks are cancelled and unwind (i.e. dropping
//! everything), that the long running operation is guaranteed to terminate. This is needed as the
//! task_scope system has no method to force spawn_blocking tasks to end/cancel, so they must handle
//! exiting themselves. For example:
//!
//! ```rust
//! {
//!     let (sender, receiver) = std::sync::mpsc::channel(10);
//!
//!     scope.spawn(async move {
//!         loop {
//!             sender.send("HELLO WORLD").unwrap();
//!             tokio::sleep(Duration::from_secs(1)).await;
//!         }
//!     });
//!
//!     scope.spawn_blocking(|| {
//!         loop {
//!             let message = receiver.recv().unwrap();
//!             println!("{}", message);
//!         }
//!     });
//!
//!     scope.spawn(async move {
//!         tokio::sleep(Duration::from_secs(100)).await;
//!         panic!();
//!         // When this panics the other `spawn()` at the top will be cancelled and unwind, which will cause
//!         // the channel sender to be dropped, so when the spawn_blocking tries to `recv()` it will panic at
//!         // the `unwrap()` with this: https://doc.rust-lang.org/std/sync/mpsc/struct.RecvError.html
//!
//!         // Of course you may wish to make the spawn_blocking in this case return an error instead of panicking, or possibly
//!         // `return Ok(())`.
//!     });
//! }
//! ```
//!
//! If you don't do the above when an error occurs the scope will not ever exit, and will wait for
//! the spawn_blocking to exit forever i.e. if the spawn_blocking was like this instead:
//!
//! ```rust
//! {
//!     scope.spawn_blocking(|| {
//!         loop {
//!             match receiver.recv() {
//!                 Ok(message) => println!("{}", message),
//!                 Err(_) => {} // We ignore the error and so the spawn_blocking will never exit of course
//!             };
//!         }
//!     });
//!
//! }
//! ```
//!
//! We should not ever use `tokio::runtime::Runtime::block_on` to avoid this [issue](https://github.com/tokio-rs/tokio/issues/4862). Also it is
//! possible for this task_scope to provide the same functionality without causing that bug to occur
//! (TODO).

use std::{
	pin::Pin,
	task::{Context, Poll},
};

use futures::{
	ready,
	stream::{FusedStream, FuturesUnordered},
	Future, Stream, StreamExt,
};
use tokio::sync::oneshot;

/// This function allows a parent task to spawn child tasks such that if any tasks panic or error,
/// all other tasks will be cancelled, and the panic or error will be propagated by this function.
/// It guarantees all tasks spawned using its scope object will finish before this function exits.
/// Thereby making accessing data outside of this scope from inside this scope via a reference safe.
/// This is why the closures/futures provided to Scope::spawn don't need static lifetimes.
pub async fn task_scope<
	'a,
	T,
	Error: Send + 'static,
	C: for<'b> FnOnce(&'b Scope<'a, Error>) -> futures::future::BoxFuture<'b, Result<T, Error>>,
>(
	parent_task: C,
) -> Result<T, Error> {
	let (scope, mut child_task_result_stream) = Scope::new();

	// try_join ensures if the parent returns an error we immediately drop child_task_result_stream
	// cancelling all children and vice versa
	tokio::try_join!(
		async move {
			while let Some(child_task_result) = child_task_result_stream.next().await {
				match child_task_result {
					Err(error) => {
						// Note we drop the child_task_result_stream on unwind causing all tasks to
						// be cancelled/aborted
						if let Ok(panic) = error.try_into_panic() {
							std::panic::resume_unwind(panic);
						} /* else: Can only occur if tokio's runtime is dropped during task
						  * scope's lifetime, in this we are about to be cancelled ourselves */
					},
					Ok(child_future_result) => child_future_result?,
				}
			}
			// child_task_result_stream has ended meaning scope has been dropped and all children
			// have finished running
			Ok(())
		},
		// This async move scope ensures scope is dropped when parent_task and its returned future
		// finish (Instead of when this function exits)
		async move { parent_task(&scope).await }
	)
	.map(|(_, t)| t)
}

type TaskFuture<Error> = Pin<Box<dyn 'static + Future<Output = Result<(), Error>> + Send>>;

/// An object used to spawn tasks into the associated scope
#[derive(Clone)]
pub struct Scope<'env, Error: Send + 'static> {
	sender: async_channel::Sender<TaskFuture<Error>>,
	/// This PhantomData pattern "&'env mut &'env ()"" is required to stop multiple
	/// spawned tasks capturing the same state and mutating it asynchronously
	/// by making the type Scope invariant wrt 'env
	_phantom: std::marker::PhantomData<&'env mut &'env ()>,
}
impl<'env, Error: Send + 'static> Scope<'env, Error> {
	fn new() -> (Self, ScopeResultStream<Error>) {
		let (sender, receiver) = async_channel::unbounded();

		(
			Scope { sender, _phantom: Default::default() },
			ScopeResultStream {
				receiver: Some(receiver),
				no_more_tasks: false,
				// Tokio has two flavors of internal runtime CurrentThread and MultiThread, I cannot
				// use the same task_scope implementation for both flavors But currently there is
				// not a nice way to detect which is being used. TODO: Once https://github.com/tokio-rs/tokio/pull/5138 is released we should use this function instead to determine the runtime variant
				#[cfg(test)]
				tasks: ScopedTasks::CurrentThread(Default::default()),
				#[cfg(not(test))]
				tasks: ScopedTasks::MultiThread(
					tokio::runtime::Handle::current(),
					Default::default(),
				),
			},
		)
	}

	pub fn spawn<F: 'env + Future<Output = Result<(), Error>> + Send>(&self, f: F) {
		let _result = self.sender.try_send({
			let future: Pin<Box<dyn 'env + Future<Output = Result<(), Error>> + Send>> =
				Box::pin(f);
			let future: TaskFuture<Error> = unsafe { std::mem::transmute(future) };
			future
		});
	}

	pub fn spawn_with_handle<
		T: Send + 'static,
		F: 'env + Future<Output = Result<T, Error>> + Send,
	>(
		&self,
		f: F,
	) -> ScopedJoinHandle<T> {
		let (handle, future) = ScopedJoinHandle::new(f);
		self.spawn(future);
		handle
	}
}

/// This struct allows code to await on the task to exit, when dropped the associated task will be
/// cancelled
pub struct ScopedJoinHandle<T> {
	receiver: oneshot::Receiver<T>,
	abort_handle: futures::future::AbortHandle,
}
impl<T> ScopedJoinHandle<T> {
	fn new<Error, F: Future<Output = Result<T, Error>> + Send>(
		f: F,
	) -> (Self, impl Future<Output = Result<(), Error>>) {
		let (sender, receiver) = oneshot::channel();
		let (abort_handle, abort_registration) = futures::future::AbortHandle::new_pair();
		let f = futures::future::Abortable::new(f, abort_registration);

		(Self { receiver, abort_handle }, async move {
			let result_aborted = f.await;

			match result_aborted {
				Ok(result_future) => match result_future {
					Ok(output) => {
						let _result = sender.send(output);
						Ok(())
					},
					Err(error) => Err(error),
				},
				Err(_) => {
					// Spawned task was aborted
					Ok(())
				},
			}
		})
	}
}
impl<T> Drop for ScopedJoinHandle<T> {
	fn drop(&mut self) {
		self.abort_handle.abort();
	}
}
impl<T> Future for ScopedJoinHandle<T> {
	type Output = T;

	fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		match Pin::new(&mut self.as_mut().receiver).poll(cx) {
			Poll::Ready(result) => match result {
				Ok(t) => Poll::Ready(t),
				Err(_) => Poll::Pending,
			},
			Poll::Pending => Poll::Pending,
		}
	}
}

enum ScopedTasks<Error: Send + 'static> {
	#[cfg(test)]
	CurrentThread(FuturesUnordered<TaskFuture<Error>>),
	// Will no longer be dead once https://github.com/tokio-rs/tokio/pull/5138 is available
	#[allow(dead_code)]
	MultiThread(
		tokio::runtime::Handle,
		FuturesUnordered<tokio::task::JoinHandle<Result<(), Error>>>,
	),
}

struct ScopeResultStream<Error: Send + 'static> {
	receiver: Option<async_channel::Receiver<TaskFuture<Error>>>,
	no_more_tasks: bool,
	tasks: ScopedTasks<Error>,
}
impl<Error: Send + 'static> Stream for ScopeResultStream<Error> {
	type Item = Result<Result<(), Error>, tokio::task::JoinError>;

	fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		while !self.no_more_tasks {
			match Pin::new(&mut self.as_mut().receiver.as_mut().unwrap()).poll_next(cx) {
				Poll::Ready(Some(future)) => {
					let tasks = &mut self.tasks;
					match tasks {
						#[cfg(test)]
						ScopedTasks::CurrentThread(tasks) => tasks.push(future),
						ScopedTasks::MultiThread(runtime, tasks) =>
							tasks.push(runtime.spawn(future)),
					}
				},
				Poll::Ready(None) => self.no_more_tasks = true,
				Poll::Pending => break,
			}
		}

		match ready!(match &mut self.tasks {
			#[cfg(test)]
			ScopedTasks::CurrentThread(tasks) => Pin::new(tasks).poll_next(cx).map(|option| option.map(Ok)),
			ScopedTasks::MultiThread(_, tasks) => Pin::new(tasks).poll_next(cx),
		}) {
			None =>
				if self.no_more_tasks {
					Poll::Ready(None)
				} else {
					Poll::Pending
				},
			out => Poll::Ready(out),
		}
	}

	fn size_hint(&self) -> (usize, Option<usize>) {
		match &self.tasks {
			#[cfg(test)]
			ScopedTasks::CurrentThread(tasks) => tasks.size_hint(),
			ScopedTasks::MultiThread(_, tasks) => tasks.size_hint(),
		}
	}
}
impl<Error: Send + 'static> FusedStream for ScopeResultStream<Error> {
	fn is_terminated(&self) -> bool {
		self.receiver.as_ref().unwrap().is_terminated() &&
			match &self.tasks {
				#[cfg(test)]
				ScopedTasks::CurrentThread(tasks) => tasks.is_terminated(),
				ScopedTasks::MultiThread(_, tasks) => tasks.is_terminated(),
			}
	}
}
impl<Error: Send + 'static> Drop for ScopeResultStream<Error> {
	fn drop(&mut self) {
		// drop all incoming spawn requests
		self.receiver = None;
		// cancel and wait for all scope's tasks to finish
		match &mut self.tasks {
			#[cfg(test)]
			ScopedTasks::CurrentThread(_) => {},
			ScopedTasks::MultiThread(runtime, tasks) =>
				if !tasks.is_empty() {
					for task in tasks.iter() {
						task.abort();
					}
					tokio::task::block_in_place(|| {
						runtime.block_on(async { while tasks.next().await.is_some() {} });
					});
				},
		}
	}
}

#[cfg(test)]
mod tests {
	use std::{convert::Infallible, sync::atomic::Ordering};

	use anyhow::anyhow;
	use futures::FutureExt;

	use super::*;

	#[tokio::main]
	#[test]
	async fn check_waits_for_tasks_to_end_when_panicking() {
		inner_check_waits_for_task_to_end(|| panic!()).await;
	}

	#[tokio::main]
	#[test]
	async fn check_waits_for_tasks_to_end_when_error() {
		inner_check_waits_for_task_to_end(|| Err(anyhow!(""))).await;
	}

	async fn inner_check_waits_for_task_to_end<
		F: Fn() -> anyhow::Result<()> + Send + Sync + 'static,
	>(
		error: F,
	) {
		// Do this a few times as tokio's scheduling of tasks is not deterministic
		// It is not possible to guarantee a spawned task has started
		for _i in 0..100 {
			const COUNT: u32 = 10;

			let task_end_count = std::sync::atomic::AtomicU32::new(0);
			let task_start_count = std::sync::atomic::AtomicU32::new(0);

			let _result = std::panic::AssertUnwindSafe(task_scope(|scope| {
				async {
					for _i in 0..COUNT {
						scope.spawn(async {
							task_start_count.fetch_add(1, Ordering::Relaxed);
							std::thread::sleep(std::time::Duration::from_millis(10));
							task_end_count.fetch_add(1, Ordering::Relaxed);
							Ok(())
						});
					}
					tokio::time::sleep(std::time::Duration::from_millis(10)).await;
					error()
				}
				.boxed()
			}))
			.catch_unwind()
			.await;

			// These aren't necessarily equal to COUNT as tokio is allowed to not start
			// spawned tasks if they have been cancelled before starting
			assert_eq!(
				task_start_count.load(Ordering::Relaxed),
				task_end_count.load(Ordering::Relaxed)
			);
		}
	}

	#[tokio::main]
	#[test]
	async fn task_handle_returns_value() {
		const VALUE: u32 = 40;
		task_scope::<_, Infallible, _>(|scope| {
			async {
				let handle = scope.spawn_with_handle(async { Ok(VALUE) });
				assert_eq!(handle.await, VALUE);
				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}

	#[tokio::main]
	#[test]
	async fn dropping_handle_cancels_task() {
		task_scope::<_, Infallible, _>(|scope| {
			async {
				let _handle = scope.spawn_with_handle::<(), _>(futures::future::pending());

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}

	#[tokio::main]
	#[test]
	async fn task_handle_does_not_return_error() {
		task_scope::<(), _, _>(|scope| {
			async {
				let handle = scope.spawn_with_handle::<(), _>(async { Err(anyhow!("")) });
				handle.await;
				panic!()
			}
			.boxed()
		})
		.await
		.unwrap_err();
	}

	#[tokio::main]
	#[test]
	async fn task_scope_ends_all_tasks_when_exiting() {
		task_scope::<_, Infallible, _>(|_scope| {
			async {
				let mut receivers = vec![];

				task_scope(|scope| {
					async {
						receivers = (0..10)
							.map(|_i| {
								let (sender, receiver) = oneshot::channel::<()>();
								scope.spawn(async move {
									let _sender = sender;
									futures::future::pending().await
								});
								receiver
							})
							.collect::<Vec<_>>();

						// Let the spawned tasks start running
						tokio::time::sleep(std::time::Duration::from_millis(10)).await;

						// Exit scope with error to cause children to be cancelled
						anyhow::Result::<()>::Err(anyhow!(""))
					}
					.boxed()
				})
				.await
				.unwrap_err();

				for receiver in &mut receivers {
					assert_eq!(receiver.try_recv(), Err(oneshot::error::TryRecvError::Closed));
				}

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}

	#[tokio::main]
	#[test]
	async fn example() {
		let mut a = 0;

		task_scope::<_, Infallible, _>(|scope| {
			async {
				scope.spawn(async {
					task_scope::<_, Infallible, _>(|scope| {
						async {
							scope.spawn(async {
								a += 10;
								Ok(())
							});
							Ok(())
						}
						.boxed()
					})
					.await
					.unwrap();

					task_scope::<_, Infallible, _>(|scope| {
						async {
							scope.spawn(async {
								a += 10;
								Ok(())
							});
							Ok(())
						}
						.boxed()
					})
					.await
					.unwrap();

					Ok(())
				});

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();

		assert_eq!(a, 20);
	}
}
