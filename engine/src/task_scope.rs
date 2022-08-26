use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures::{ready, stream::FuturesUnordered, Future, Stream, StreamExt};
use futures_core::FusedStream;
use tokio::{
    sync::{mpsc, oneshot},
    task::{JoinError, JoinHandle},
};

/*
# Task Scope

The idea here is very similiar to the [thread_scope](https://doc.rust-lang.org/1.63.0/std/thread/fn.scope.html) feature in the std library.
The important differences being it:
    - is designed to work with futures
    - propagates errors returned by tasks ([instead of only panics](https://doc.rust-lang.org/1.63.0/std/thread/fn.scope.html#panics))
    - when tasks panic or return errors, this will cause all other still running tasks to be [cancelled](https://blog.yoshuawuyts.com/async-cancellation-1/)

A scope is designed to allow you to spawn asynchronous tasks, wait for all those tasks to finished, and handle errors/panics caused by those tasks.

When you create a scope, you must provide a parent task/"async closure", which is passed a handle via which
you can spawn further child tasks, which run asychronously to the parent task. The scope will not exit/return
until all the tasks have completed. Iff any of the scope's tasks panic or return an error, the scope will
cancel all remaining tasks, and end by respectively panicking or returning the error (For "with_task_scope",
in this case the scope will not wait for all tasks to complete).

The reason "with_task_scope" does not wait for all tasks to complete in the error/panic case,
is that this is not currently possible in all cases (and it doesn't really matter) given the way futures work. For more information
look into [AsyncDrop](https://rust-lang.github.io/async-fundamentals-initiative/roadmap/async_drop.html).

For the public functions in this module, if they are used incorrectly the code will not compile.

# Usage

`scope.spawn()` should be used instead of `tokio::spawn()`.

`scope.spawn_blocking()` (To be added) should be used instead of `tokio::spawn_blocking()` unless you are running an operation that is
guaranteed to exit after a finite period, and you are awaiting on the JoinHandle immediately after spawning. This exception is
made as in this case the task_scope system offers no advantage, and may make the code more complex as you need to pass around a scope.
TODO: Possibly introduce a function to express this exception.

Where `scope.spawn_blocking()` is used for a long running operations the developer must ensure that if the rest of the non-spawn-blocking tasks are cancelled
and unwind (i.e. dropping everything), that the long running operation is guaranteed to terminate. For example:

```rust
{
    let (sender, receiver) = std::sync::mpsc::channel(10);

    scope.spawn(async move {
        loop {
            sender.send("HELLO WORLD").unwrap();
            tokio::sleep(Duration::from_secs(1)).await;
        }
    });

    scope.spawn_blocking(|| {
        loop {
            let message = receiver.recv().unwrap();
            println!("{}", message);
        }
    });

    scope.spawn(async move {
        tokio::sleep(Duration::from_secs(100)).await;
        panic!();
        // When this panics the other `spawn()` at the top will be cancelled and unwind, which will cause
        // the channel sender to be dropped, so when the spawn_blocking tries to `recv()` it will panic at
        // the `unwrap()` with this: https://doc.rust-lang.org/std/sync/mpsc/struct.RecvError.html

        // Of course you may wish to make the spawn_blocking in this case return an error instead of panicking, or possibly
        // `return Ok(())`.
    });
}
```

If you don't do the above when an error occurs the scope will not ever exit, and will wait for the spawn_blocking to exit
forever i.e. if the spawn_blocking was like this instead:

```
{
    scope.spawn_blocking(|| {
        loop {
            match receiver.recv() {
                Ok(message) => println!("{}", message),
                Err(_) => {} // We ignore the error and so the spawn_blocking will never exit of course
            };
        }
    });

}
```
*/

/// Allows a parent closure/future to spawn child tasks, such that if the parent or child fail, they
/// will all be cancelled, and the panic/Error will be propagated by this function.
/// Note: This function is unsafe as if the function is called with TASKS_HAVE_STATIC_LIFETIMES=false
/// and the call to this async function is "cancelled" it may cause spawned tasks to do invalid memory accesses
async unsafe fn inner_with_task_scope<
    'env,
    C: for<'scope> FnOnce(
        &'scope Scope<'env, anyhow::Result<()>, TASKS_HAVE_STATIC_LIFETIMES>,
    ) -> futures::future::BoxFuture<'scope, anyhow::Result<T>>, // Box is needed to link the lifetime of the reference passed to the closure to the lifetime of the returned future
    T,
    const TASKS_HAVE_STATIC_LIFETIMES: bool,
>(
    parent_task: C,
) -> anyhow::Result<T> {
    let (scope, mut child_task_result_stream) = new_task_scope();

    // try_join ensures if the parent returns an error we immediately drop child_task_result_stream cancelling all children and vice versa
    tokio::try_join!(
        async move {
            while let Some(child_task_result) = child_task_result_stream.next().await {
                match child_task_result {
                    Err(error) => {
                        if let Ok(reason) = error.try_into_panic() {
                            // Note we drop the child_task_result_stream on unwind causing all tasks to be cancelled/aborted
                            std::panic::resume_unwind(reason)
                        } else {
                            panic!(
                                "THERE IS A MISTAKE IN THE CALLING CODE IF THIS HAPPENS. \
                                The tokio runtime has been dropped causing child tasks to be cancelled. \
                                This can only happen if the runtime was dropped before this function finished, \
                                which should be impossible if all tasks are spawned via this mechanism \
                                and the runtime is not manually dropped."
                            )
                        }
                    }
                    Ok(child_future_result) => child_future_result?,
                }
            }
            // child_task_result_stream has ended meaning scope has been dropped and all children have finished running
            Ok(())
        },
        // This async scope ensures scope is dropped when c and its returned future finish (Instead of when this function exits)
        async move {
            parent_task(&scope).await
        }
    ).map(|(_, t)| t)
}

fn new_task_scope<'env, TaskResult, const TASKS_HAVE_STATIC_LIFETIMES: bool>() -> (
    Scope<'env, TaskResult, TASKS_HAVE_STATIC_LIFETIMES>,
    ScopeResultStream<TaskResult>,
) {
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();

    (
        Scope {
            spawner: tokio::runtime::Handle::current(),
            sender,
            _phantom: Default::default(),
        },
        ScopeResultStream {
            receiver,
            receiver_closed: false,
            join_handles: Default::default(),
        },
    )
}

/// When this object is dropped it will cancel/abort the associated tokio task
/// The tokio task will continue to run after the cancel/abort until it hits an await.
struct CancellingJoinHandle<T> {
    handle: JoinHandle<T>,
}
impl<T> CancellingJoinHandle<T> {
    fn new(handle: JoinHandle<T>) -> Self {
        Self { handle }
    }
}
impl<T> Drop for CancellingJoinHandle<T> {
    fn drop(&mut self) {
        self.handle.abort();
    }
}
impl<T> Future for CancellingJoinHandle<T> {
    type Output = <JoinHandle<T> as Future>::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        unsafe { Pin::new_unchecked(&mut self.handle) }.poll(cx)
    }
}

/// An object used to spawn tasks into the associated scope
/// The spawned task's futures are either required to have a
/// static lifetime if TASKS_HAVE_STATIC_LIFETIMES, otherwise
/// they are required to have a lifetime of 'env
pub struct Scope<'env, T, const TASKS_HAVE_STATIC_LIFETIMES: bool> {
    spawner: tokio::runtime::Handle,
    sender: mpsc::UnboundedSender<CancellingJoinHandle<T>>,
    /// This PhantomData pattern "&'env mut &'env ()"" is required to stop multiple
    /// spawned tasks capturing the same state and mutating it asynchronously
    /// by making the type Scope invariant wrt 'env
    _phantom: std::marker::PhantomData<&'env mut &'env ()>,
}

/// This struct allows code to await on the task to exit
pub struct ScopedJoinHandle<T> {
    receiver: oneshot::Receiver<T>,
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

/// A stream of spawned task exit reasons (Ok, Err, or panic)
/// This stream will only end once the associated Scope object is dropped
struct ScopeResultStream<T> {
    receiver: mpsc::UnboundedReceiver<CancellingJoinHandle<T>>,
    receiver_closed: bool,
    join_handles: FuturesUnordered<CancellingJoinHandle<T>>,
}

impl<T> Stream for ScopeResultStream<T> {
    type Item = Result<T, JoinError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        while !self.receiver_closed {
            match Pin::new(&mut self.as_mut().receiver).poll_recv(cx) {
                Poll::Pending => break,
                Poll::Ready(None) => self.receiver_closed = true,
                Poll::Ready(Some(handle)) => self.join_handles.push(handle),
            }
        }

        match ready!(Pin::new(&mut self.as_mut().join_handles).poll_next(cx)) {
            None if self.receiver_closed => Poll::Ready(None),
            None => Poll::Pending,
            out => Poll::Ready(out),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.join_handles.size_hint()
    }
}

impl<T> FusedStream for ScopeResultStream<T> {
    fn is_terminated(&self) -> bool {
        self.receiver_closed && self.join_handles.is_terminated()
    }
}

macro_rules! impl_spawn_ops {
    ($env_lifetime:lifetime, $stat:literal, $task_lifetime:lifetime) => {
        impl<$env_lifetime, T: 'static + Send> Scope<$env_lifetime, T, $stat> {
            // The returned handle should only ever be awaited on inside of the task scope
            // this spawn is associated with, or any sub-task scopes. Otherwise the await
            // will never complete in the Error case.
            fn spawn_with_custom_error_handling<
                R,
                V: 'static + Send,
                F: $task_lifetime + Future<Output = R> + Send,
                ErrorHandler: $task_lifetime + FnOnce(R) -> (T, Option<V>) + Send,
            >(
                &self,
                error_handler: ErrorHandler,
                f: F,
            ) -> ScopedJoinHandle<V> {
                let (sender, receiver) = oneshot::channel();

                self.spawn(async move {
                    let result = f.await;

                    let (t, option_v) = error_handler(result);

                    if let Some(v) = option_v {
                        let _result = sender.send(v);
                    }

                    t
                });

                ScopedJoinHandle { receiver }
            }
        }

        impl<$env_lifetime> Scope<$env_lifetime, anyhow::Result<()>, $stat> {
            /// Spawns a task and gives you a handle to receive the Result::Ok of the task by awaiting
            /// on the returned handle (If the task errors/panics the scope will exit with the error/panic)
            pub fn spawn_with_handle<
                V: 'static + Send,
                F: $task_lifetime + Future<Output = anyhow::Result<V>> + Send,
            >(
                &self,
                f: F,
            ) -> ScopedJoinHandle<V> {
                self.spawn_with_custom_error_handling(
                    |t| match t {
                        Ok(v) => (Ok(()), Some(v)),
                        Err(e) => (Err(e), None),
                    },
                    f,
                )
            }

            /// Spawns a task and gives you a handle to receive the Result of the task by awaiting
            /// on the returned handle (If the task panics the scope will exit by panicking, but
            /// if the task returns an error the scope will not exit)
            pub fn spawn_with_manual_error_handling<
                V: 'static + Send,
                F: $task_lifetime + Future<Output = anyhow::Result<V>> + Send,
            >(
                &self,
                f: F,
            ) -> ScopedJoinHandle<anyhow::Result<V>> {
                self.spawn_with_custom_error_handling(|t| (Ok(()), Some(t)), f)
            }
        }
    };
}

/// This function allows a parent task to spawn child tasks such that if any tasks panic or error,
/// all other tasks will be cancelled, and the panic or error will be propagated by this function.
/// It guarantees all tasks spawned using its scope object will finish before this function exits.
/// Thereby making accessing data outside of this scope from inside this scope via a reference safe.
/// This is why the closures/futures provided to Scope::spawn don't need static lifetimes.
#[tokio::main]
pub async fn with_main_task_scope<
    'env,
    C: for<'scope> FnOnce(
        &'scope Scope<'env, anyhow::Result<()>, false>,
    ) -> futures::future::BoxFuture<'scope, anyhow::Result<T>>,
    T,
>(
    parent_task: C,
) -> anyhow::Result<T> {
    // Safe as the provided future (via closure) is never cancelled
    unsafe { inner_with_task_scope(parent_task).await }
}

impl<'env, T: Send + 'static> Scope<'env, T, false> {
    /// Spawn a task that is guaranteed to exit or cancel/abort before the associated scope exits
    pub fn spawn<F: 'env + Future<Output = T> + Send>(&self, f: F) {
        // If this result is an error (I.e. the channel receiver was dropped then the stream was closed, and so we want to drop the handle to cancel the task we just spawned)
        let _result = self.sender.send(CancellingJoinHandle::new({
            let future: Pin<Box<dyn 'env + Future<Output = T> + Send>> = Box::pin(f);
            let future: Pin<Box<dyn 'static + Future<Output = T> + Send>> =
                unsafe { std::mem::transmute(future) };
            self.spawner.spawn(future)
        }));
    }
}

impl_spawn_ops!('env, false, 'env);

/// This function allows a parent task to spawn child tasks such that if any tasks panic or error,
/// all other tasks will be cancelled.
/// Unlike with_main_task_scope this doesn't guarantee all child tasks have finished running once
/// this function exists, only that have will have been cancelled. This is why child tasks must
/// have static lifetimes.
pub async fn with_task_scope<
    'a,
    C: for<'b> FnOnce(
        &'b Scope<'a, anyhow::Result<()>, true>,
    ) -> futures::future::BoxFuture<'b, anyhow::Result<T>>,
    T,
>(
    parent_task: C,
) -> anyhow::Result<T> {
    // Safe as closures/futures are forced to have static lifetimes
    unsafe { inner_with_task_scope(parent_task).await }
}

impl<'a, T: Send + 'static> Scope<'a, T, true> {
    /// Spawn a task that is guaranteed to exit/cancel/abort before the associated scope exits
    pub fn spawn<F: 'static + Future<Output = T> + Send>(&self, f: F) {
        // If this result is an error (I.e. the channel receiver was dropped) then the stream was closed, and so we want to drop the handle to cancel the task we just spawned.
        let _result = self
            .sender
            .send(CancellingJoinHandle::new(self.spawner.spawn(Box::pin(f))));
    }
}

impl_spawn_ops!('env, true, 'static);

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use anyhow::anyhow;
    use futures::FutureExt;
    use utilities::assert_err;

    use super::*;

    async fn wait_forever() {
        let (_sender, receiver) = oneshot::channel::<()>();
        let _result = receiver.await;
    }

    #[test]
    fn check_waits_for_tasks_to_end_when_panicking() {
        inner_check_waits_for_task_to_end(|| panic!());
    }

    #[test]
    fn check_waits_for_tasks_to_end_when_error() {
        inner_check_waits_for_task_to_end(|| Err(anyhow!("")));
    }

    fn inner_check_waits_for_task_to_end<F: Fn() -> anyhow::Result<()> + Send + Sync + 'static>(
        error: F,
    ) {
        // Do this a few times as tokio's scheduling of tasks is not deterministic
        // It is not possible to guarantee a spawned task has started
        for _i in 0..100 {
            const COUNT: u32 = 10;

            let task_end_count = std::sync::atomic::AtomicU32::new(0);
            let task_start_count = std::sync::atomic::AtomicU32::new(0);

            let _result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> anyhow::Result<()> {
                    with_main_task_scope(|scope| {
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
                    })
                }))
                .map(|result| result.unwrap_err()); // with_main_task_scope should either panic or error

            // These aren't necessarily equal to COUNT as tokio is allowed to not start
            // spawned tasks if they have been cancelled before starting
            assert_eq!(
                task_start_count.load(Ordering::Relaxed),
                task_end_count.load(Ordering::Relaxed)
            );
        }
    }

    #[test]
    fn join_handles_return_value_correctly() {
        const VALUE: u32 = 40;
        with_main_task_scope(|scope| {
            async {
                let handle = scope.spawn_with_handle(async { Ok(VALUE) });

                assert_eq!(handle.await, VALUE);

                Ok(())
            }
            .boxed()
        })
        .unwrap();
    }

    #[test]
    fn join_handles_handle_errors() {
        with_main_task_scope::<'_, _, ()>(|scope| {
            async {
                let handle = scope.spawn_with_handle::<(), _>(async { Err(anyhow!("")) });

                handle.await;
                unreachable!()
            }
            .boxed()
        })
        .unwrap_err();
    }

    #[test]
    fn task_scope_cancels_all_tasks_when_exiting() {
        with_main_task_scope(|_scope| {
            async {
                let mut receivers = vec![];

                with_task_scope(|scope| {
                    async {
                        receivers = (0..10)
                            .map(|_i| {
                                let (sender, receiver) = oneshot::channel::<()>();
                                scope.spawn(async move {
                                    let _sender = sender;
                                    wait_forever().await;
                                    Ok(())
                                });
                                receiver
                            })
                            .collect::<Vec<_>>();

                        // Exit scope with error to cause children to be cancelled
                        anyhow::Result::<()>::Err(anyhow!(""))
                    }
                    .boxed()
                })
                .await
                .unwrap_err();

                for receiver in receivers {
                    assert_err!(receiver.await);
                }

                Ok(())
            }
            .boxed()
        })
        .unwrap();
    }

    #[tokio::test]
    async fn cancelling_join_handle() {
        let (sender, receiver) = oneshot::channel::<()>();
        let handle = CancellingJoinHandle::new(tokio::spawn(async move {
            let _sender = sender; // move into task
            wait_forever().await;
        }));

        drop(handle);

        receiver.await.unwrap_err(); // we expect sender to be dropped when task is cancelled
    }
}
