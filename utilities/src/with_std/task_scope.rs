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
//! When you create a scope, you must provide a top level task/"async closure", which is passed a
//! handle via which you can spawn further tasks which run asychronously.
//! The scope will not exit/return until all the tasks have completed or been cancelled. Iff any of
//! the scope's tasks panic or return an error, the scope will cancel all remaining tasks, and end
//! by respectively panicking or returning the error.
//!
//! For the public functions in this module, if they are used unsafely the code will not compile.
//!
//! # Usage
//!
//! `scope.spawn()/scope.spawn_weak()/scope.spawn_with_handle()` should be used instead of
//! `tokio::spawn()`.
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
//! ```rust(ignore)
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
//! ```rust(ignore)
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

use core::fmt::Debug;
use futures::{
	ready,
	stream::{FusedStream, FuturesUnordered},
	Future, FutureExt, Stream, StreamExt,
};
use tokio::sync::oneshot;

pub trait Unwrappable {
	type Item;

	fn __internal_to_option(x: Self) -> Option<Self::Item>;
}
impl<T> Unwrappable for Option<T> {
	type Item = T;

	fn __internal_to_option(x: Self) -> Option<Self::Item> {
		x
	}
}
impl<T, E> Unwrappable for Result<T, E> {
	type Item = T;

	fn __internal_to_option(x: Self) -> Option<Self::Item> {
		x.ok()
	}
}

const UNWRAP_OR_CANCEL_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500u64);

#[pin_project::pin_project]
pub struct UnwrapOrCancelFuture<F> {
	#[pin]
	f: Option<F>,
	#[pin]
	timeout: Option<tokio::time::Sleep>,
}
impl<T> Future for UnwrapOrCancelFuture<T>
where
	T: Future,
	T::Output: Unwrappable,
{
	type Output = <T::Output as Unwrappable>::Item;

	fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		let mut this = self.project();
		if let Some(f) = this.f.as_mut().as_pin_mut() {
			if let Some(output) = Unwrappable::__internal_to_option(ready!(f.poll(cx))) {
				return Poll::Ready(output)
			} else {
				// Avoids possible deadlocks during sleep (And avoids polling again after ready)
				this.f.set(None);
			}
		}

		if this.timeout.is_none() {
			this.timeout.set(Some(tokio::time::sleep(UNWRAP_OR_CANCEL_TIMEOUT)));
		}
		ready!(this.timeout.as_pin_mut().unwrap().poll(cx));
		// We only reach this panic if the sleep is ready/ended
		panic!("Expected task to be cancelled due to another task's failure, but it was not cancelled within {UNWRAP_OR_CANCEL_TIMEOUT:?}");
	}
}

pub trait UnwrapOrCancel {
	/// This expresses the idea that this unwrap can only fail if another task has failed (errored
	/// or panicked) thereby causing the task scope to be cancelled, and so we expect this task to
	/// be cancelled very shortly if this unwrap fails.
	fn unwrap_or_cancel(self) -> UnwrapOrCancelFuture<Self>
	where
		Self: Future + Sized,
		Self::Output: Unwrappable,
	{
		UnwrapOrCancelFuture { f: Some(self), timeout: None }
	}
}
impl<T: ?Sized> UnwrapOrCancel for T where T: Future {}

/// This function allows a top level task to spawn tasks such that if any tasks panic or error,
/// all other tasks will be cancelled, and the panic or error will be propagated by this function.
/// It guarantees all tasks spawned using its scope object will finish before this function exits.
/// Thereby making accessing data outside of this scope from inside this scope via a reference safe.
/// This is why the closures/futures provided to Scope::spawn don't need static lifetimes.
#[track_caller]
pub fn task_scope<
	'a,
	T,
	Error: Debug + Send + 'static,
	C: for<'b> FnOnce(&'b Scope<'a, Error>) -> futures::future::BoxFuture<'b, Result<T, Error>>,
>(
	top_level_task: C,
) -> impl Future<Output = Result<T, Error>> {
	let location = core::panic::Location::caller();

	async move {
		tracing::info!(target: "task_scope", "opened: '{location}'");
		let guard = scopeguard::guard((), move |_| {
			if std::thread::panicking() {
				tracing::error!(target: "task_scope", "closed by panic: '{location}'");
			} else {
				tracing::error!(target: "task_scope", "closed by cancellation: '{location}'");
			}
		});

		let (scope, mut task_result_stream) = Scope::new();

		// try_join ensures if the top level task returns an error we immediately drop
		// `task_result_stream`, which in turn cancels all the tasks
		let result = tokio::try_join!(
			async move {
				while let Some(task_result) = task_result_stream.next().await {
					match task_result {
						Err(error) => {
							// Note we drop the task_result_stream on unwind causing all tasks to
							// be cancelled/aborted
							if let Ok(panic) = error.try_into_panic() {
								std::panic::resume_unwind(panic);
							} /* else: Can only occur if tokio's runtime is dropped during task
							  * scope's lifetime, in this case we are about to be cancelled
							  * ourselves */
						},
						Ok(future_result) => future_result?,
					}
				}
				// task_result_stream has ended meaning scope has been dropped and all tasks
				// (excluding the top-level task) have finished running
				Ok(())
			},
			// This async move scope ensures scope is dropped when top_level_task and its returned
			// future finish (Instead of when this function exits)
			async move {
				tracing::info!(target: "task_scope", "parent task started: '{location}'");
				let guard = scopeguard::guard((), move |_| {
					if std::thread::panicking() {
						tracing::error!(target: "task_scope", "parent task ended by panic: '{location}'");
					} else {
						tracing::error!(target: "task_scope", "parent task ended by cancellation: '{location}'");
					}
				});
				let result = top_level_task(&scope).await;
				scopeguard::ScopeGuard::into_inner(guard);
				match &result {
					Ok(_) =>
						tracing::info!(target: "task_scope", "parent task ended: '{location}'"),
					Err(error) =>
						tracing::error!(target: "task_scope", "parent task ended by error '{error:?}': '{location}'"),
				}
				result
			}
		);

		scopeguard::ScopeGuard::into_inner(guard);

		match result {
			Ok((_, t)) => {
				tracing::info!(target: "task_scope", "closed: '{location}'", );
				Ok(t)
			},
			Err(error) => {
				tracing::error!(target: "task_scope", "closed by error {error:?}: '{location}'");
				Err(error)
			},
		}
	}
}

type TaskFuture<Error> = Pin<Box<dyn 'static + Future<Output = Result<(), Error>> + Send>>;

#[derive(Clone, Copy)]
struct TaskProperties {
	weak: bool,
	location: core::panic::Location<'static>,
}
impl TaskProperties {
	fn log_on_end<T, E: Debug + Send + 'static>(
		&self,
		result: &Result<Result<T, E>, tokio::task::JoinError>,
	) {
		match &result {
			Ok(result) => match result {
				Ok(_) =>
					tracing::info!(target: "task_scope", "child task ended: '{}'", self.location),
				Err(error) =>
					tracing::error!(target: "task_scope", "child task ended by error '{error:?}': '{}'", self.location),
			},
			Err(error) =>
				if error.is_panic() {
					tracing::error!(target: "task_scope", "child task ended by panic: '{}'", self.location);
				} else {
					tracing::error!(target: "task_scope", "child task ended by cancellation: '{}'", self.location);
				},
		}
	}
}

#[pin_project::pin_project]
struct TaskWrapper<Task> {
	#[pin]
	future: Task,
	properties: TaskProperties,
}
impl<Task: Future + Unpin + 'static> Future for TaskWrapper<Task> {
	type Output = (TaskProperties, Task::Output);

	fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		Pin::new(&mut self.future).poll(cx).map(|output| (self.properties, output))
	}
}

/// An object used to spawn tasks into the associated scope
pub struct Scope<'env, Error: Debug + Send + 'static> {
	sender: async_channel::Sender<(TaskProperties, TaskFuture<Error>)>,
	/// Invariance over 'env, to make sure 'env cannot shrink,
	/// which is necessary for soundness.
	///
	/// Without invariance, this would compile fine but be unsound:
	///
	/// ```compile_fail,E0373
	/// use utilities::task_scope::task_scope;
	/// use futures::FutureExt;
	///
	/// let mut a = 1;
	/// task_scope::<(), (), _>(|scope| async move {
	///     scope.spawn(async {
	///         a += 1;
	///             Ok(())
	///     });
	///     scope.spawn(async {
	///         a += 1; // might run concurrently to other spawn
	///             Ok(())
	///     });
	///
	///             Ok(())
	/// }.boxed());
	/// ```
	_phantom: std::marker::PhantomData<&'env mut &'env ()>,
}
impl<'env, Error: Debug + Send + 'static> Scope<'env, Error> {
	fn new() -> (Self, ScopeResultStream<Error>) {
		// Must be unbounded so that `try_send` in `spawn` will only fail if the receiver is
		// dropped, meaning the scope is exiting/aborting, and not when it is full
		let (sender, receiver) = async_channel::unbounded();

		let runtime_handle = tokio::runtime::Handle::current();

		(
			Scope { sender, _phantom: Default::default() },
			ScopeResultStream {
				receiver: Some(receiver),
				can_receive_new_tasks: true,
				non_weak_tasks: 0,
				tasks: match runtime_handle.runtime_flavor() {
					tokio::runtime::RuntimeFlavor::CurrentThread =>
						ScopedTasks::CurrentThread(Default::default()),
					tokio::runtime::RuntimeFlavor::MultiThread => ScopedTasks::MultiThread(
						tokio::runtime::Handle::current(),
						Default::default(),
					),
					flavor =>
						unimplemented!("Unknown runtime flavor '{:?}' is not supported", flavor),
				},
			},
		)
	}

	#[track_caller]
	fn inner_spawn<F: 'env + Future<Output = Result<(), Error>> + Send>(&self, weak: bool, f: F) {
		let location = core::panic::Location::caller();
		let _result = self.sender.try_send({
			let future: Pin<Box<dyn 'env + Future<Output = Result<(), Error>> + Send>> =
				Box::pin(f);
			let future: TaskFuture<Error> = unsafe { std::mem::transmute(future) };
			(TaskProperties { weak, location: *location }, future)
		});
	}

	/// Spawns a task that the scope will wait for before exiting.
	#[track_caller]
	pub fn spawn<F: 'env + Future<Output = Result<(), Error>> + Send>(&self, f: F) {
		self.inner_spawn(false, f)
	}

	/// Spawns a task that the scope will not wait for before exiting, instead it will be cancelled.
	#[track_caller]
	pub fn spawn_weak<F: 'env + Future<Output = Result<(), Error>> + Send>(&self, f: F) {
		self.inner_spawn(true, f)
	}

	/// Spawns a task that the scope will wait for before exiting, and returns a handle that you can
	/// receive the output of the task.
	#[track_caller]
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

		(
			Self { receiver, abort_handle },
			f.map(move |result_aborted| {
				match result_aborted {
					Ok(result_future) => result_future.map(move |t| {
						let _result = sender.send(t);
					}),
					Err(_) => {
						// Spawned task was aborted
						Ok(())
					},
				}
			}),
		)
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
				Err(_) => Poll::Pending, /* Await forever. This is ok as this means the
				                          * associated task returned an
				                          * error, and so the task_scope is
				                          * exiting/aborting, and so where we are awaiting on
				                          * this future, it is going to
				                          * be cancelled (TODO: Add lifetime
				                          * to ScopedJoinHandle to guarantee ScopedJoinHandle
				                          * cannot be await'ed on outside of its associated
				                          * task_scope) */
			},
			Poll::Pending => Poll::Pending,
		}
	}
}

enum ScopedTasks<Error: Debug + Send + 'static> {
	CurrentThread(FuturesUnordered<TaskWrapper<TaskFuture<Error>>>),
	MultiThread(
		tokio::runtime::Handle,
		FuturesUnordered<TaskWrapper<tokio::task::JoinHandle<Result<(), Error>>>>,
	),
}

struct ScopeResultStream<Error: Debug + Send + 'static> {
	receiver: Option<async_channel::Receiver<(TaskProperties, TaskFuture<Error>)>>,
	can_receive_new_tasks: bool,
	non_weak_tasks: usize,
	tasks: ScopedTasks<Error>,
}
impl<Error: Debug + Send + 'static> Stream for ScopeResultStream<Error> {
	type Item = Result<Result<(), Error>, tokio::task::JoinError>;

	fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		if self.can_receive_new_tasks {
			while let Poll::Ready(option) =
				Pin::new(&mut self.as_mut().receiver.as_mut().unwrap()).poll_next(cx)
			{
				if let Some((properties, future)) = option {
					tracing::info!(target: "task_scope", "child task started: '{}'", properties.location);
					if !properties.weak {
						self.non_weak_tasks += 1;
					}
					let tasks = &mut self.tasks;
					match tasks {
						ScopedTasks::CurrentThread(tasks) =>
							tasks.push(TaskWrapper { future, properties }),
						ScopedTasks::MultiThread(runtime, tasks) =>
							tasks.push(TaskWrapper { future: runtime.spawn(future), properties }),
					}
				} else {
					// Sender/`Scope` has been dropped
					self.can_receive_new_tasks = false;
					break
				}
			}
		}

		if !self.can_receive_new_tasks && self.non_weak_tasks == 0 {
			Poll::Ready(None)
		} else {
			match ready!(match &mut self.tasks {
				ScopedTasks::CurrentThread(tasks) => Pin::new(tasks)
					.poll_next(cx)
					.map(|option| option.map(|(properties, result)| (properties, Ok(result)))),
				ScopedTasks::MultiThread(_, tasks) => Pin::new(tasks).poll_next(cx),
			}) {
				None =>
					if self.can_receive_new_tasks {
						Poll::Pending
					} else {
						Poll::Ready(None)
					},
				Some((properties, result)) => {
					properties.log_on_end(&result);
					if !properties.weak {
						self.non_weak_tasks -= 1;
					}
					Poll::Ready(Some(result))
				},
			}
		}
	}

	fn size_hint(&self) -> (usize, Option<usize>) {
		match &self.tasks {
			ScopedTasks::CurrentThread(tasks) => tasks.size_hint(),
			ScopedTasks::MultiThread(_, tasks) => tasks.size_hint(),
		}
	}
}
impl<Error: Debug + Send + 'static> FusedStream for ScopeResultStream<Error> {
	fn is_terminated(&self) -> bool {
		self.receiver.as_ref().unwrap().is_terminated() &&
			match &self.tasks {
				ScopedTasks::CurrentThread(tasks) => tasks.is_terminated(),
				ScopedTasks::MultiThread(_, tasks) => tasks.is_terminated(),
			}
	}
}
impl<Error: Debug + Send + 'static> Drop for ScopeResultStream<Error> {
	fn drop(&mut self) {
		// drop all incoming spawn requests
		self.receiver = None;
		// cancel and wait for all scope's tasks to finish
		match &mut self.tasks {
			// Tokio has several flavors of internal runtime
			// tokio::task::block_in_place doesn't work in a CurrentThread runtime.
			ScopedTasks::CurrentThread(tasks) => {
				// We don't need to wait for tasks to finish here as the tasks member contains all
				// the futures, so once we drop `tasks` we know all the spawned futures are gone.
				// Whereas in the MultiThread case calling abort() doesn't guarantee the spawned
				// futures as gone.

				for task in tasks.into_iter() {
					tracing::error!(target: "task_scope", "child task ended by cancellation: '{}'", task.properties.location);
				}
			},
			ScopedTasks::MultiThread(runtime, tasks) =>
				if !tasks.is_empty() {
					for task in tasks.iter() {
						task.future.abort();
					}
					tokio::task::block_in_place(|| {
						runtime.block_on(tasks.for_each(|(properties, result)| {
							properties.log_on_end(&result);
							async {}
						}));
					});
				},
		}
	}
}

/// Allows async code to run sync/blocking code without blocking the runtime.
pub async fn without_blocking<C: FnOnce() -> R + Send + 'static, R: Send + 'static>(c: C) -> R {
	match tokio::task::spawn_blocking(c).await {
		Ok(r) => r,
		Err(join_error) =>
			if let Ok(panic) = join_error.try_into_panic() {
				// Avoids re-printing panics that occur inside the spawned code, which would occur
				// if unwrap() was used.
				std::panic::resume_unwind(panic)
			} else {
				// Silently wait to be cancelled.
				futures::future::pending().await
			},
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

	#[tokio::main]
	#[test]
	async fn scope_doesnt_wait_for_weak_tasks() {
		task_scope::<_, Infallible, _>(|scope| {
			async {
				scope.spawn_weak(futures::future::pending());

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}

	#[tokio::test]
	async fn test_unwrap_or_cancel() {
		async fn inner<T: Unwrappable + Clone>(some: T, none: T)
		where
			T::Item: Debug,
		{
			crate::assert_future_panics!(async { none.clone() }.unwrap_or_cancel());

			let shorter_than_timeout: std::time::Duration =
				UNWRAP_OR_CANCEL_TIMEOUT.mul_f64(0.5f64);

			crate::assert_err!(
				tokio::time::timeout(
					shorter_than_timeout,
					async { none.clone() }.unwrap_or_cancel()
				)
				.await
			);

			let longer_than_timeout: std::time::Duration = UNWRAP_OR_CANCEL_TIMEOUT.mul_f64(2.0f64);

			crate::assert_future_panics!(tokio::time::timeout(
				longer_than_timeout,
				async { none.clone() }.unwrap_or_cancel()
			));

			async { some.clone() }.unwrap_or_cancel().await;

			crate::assert_ok!(
				tokio::time::timeout(
					shorter_than_timeout,
					async { some.clone() }.unwrap_or_cancel()
				)
				.await
			);
			crate::assert_ok!(
				tokio::time::timeout(
					shorter_than_timeout,
					async { some.clone() }.unwrap_or_cancel()
				)
				.await
			);
		}

		inner(Some(()), None).await;
		inner(Ok(()), Err(())).await;
	}
}
