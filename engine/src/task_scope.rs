use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures::{ready, stream::FuturesUnordered, Future, FutureExt, Stream, StreamExt};
use futures_core::FusedStream;
use tokio::{
    runtime::Handle,
    sync::{
        mpsc::{UnboundedReceiver, UnboundedSender},
        oneshot::{self, Receiver},
    },
    task::{JoinError, JoinHandle},
};

async unsafe fn inner_with_task_scope<
    'a,
    C: for<'b> FnOnce(
        &'b Scope<'a, anyhow::Result<()>, STATIC>,
    ) -> futures::future::BoxFuture<'b, anyhow::Result<T>>,
    T,
    const STATIC: bool,
>(
    c: C,
) -> anyhow::Result<T> {
    let (scope, mut scope_result_stream) = new_task_scope();

    let (result, main_result) = tokio::join!(
        async move {
            while let Some(thread_result) = scope_result_stream.next().await {
                match thread_result {
                    Err(error) => {
                        if let Ok(reason) = error.try_into_panic() {
                            std::panic::resume_unwind(reason)
                        } else {
                            panic!(
                                "THERE IS A MISTAKE IN THE CALLING CODE IF THIS HAPPENS. \
                                The tokio runtime has been dropped causing spawned task to be cancelled. \
                                This can only happen if the runtime was dropped before this call finished, \
                                which should be impossible if all tasks are spawned via this mechanism \
                                and the runtime is not manually dropped."
                            )
                        }
                    }
                    Ok(future_result) => future_result?,
                }
            }
            Ok(())
        },
        std::panic::AssertUnwindSafe(async move {
            // async scope ensures scope is dropped when c finished or panics
            c(&scope).await
        })
        .catch_unwind() // Ensures we join all spawned tasks before resuming unwind
    );

    result.and(match main_result {
        Ok(main_result) => main_result,
        Err(panic) => std::panic::resume_unwind(panic),
    })
}

fn new_task_scope<'a, TaskResult, const STATIC: bool>(
) -> (Scope<'a, TaskResult, STATIC>, ScopeResultStream<TaskResult>) {
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

/// When this object is dropped it will cancel/abort the associated tokio thread
/// The tokio thread will continue to run after the cancel/abort until it hits an await.
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

/// An object used to spawn threads into the associated scope
pub struct Scope<'a, T, const STATIC: bool> {
    spawner: Handle,
    sender: UnboundedSender<CancellingJoinHandle<T>>,
    _phantom: std::marker::PhantomData<&'a mut &'a ()>,
}

/// This allows code to spawn a thread, and await on the thread to exit by await'ing on the thread's ScopedJoinHandle
pub struct ScopedJoinHandle<T> {
    receiver: Receiver<T>,
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

// A stream of spawned thread exit reasons (Ok, Err, panic)
struct ScopeResultStream<T> {
    receiver: UnboundedReceiver<CancellingJoinHandle<T>>,
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
    ($env_lifetime:lifetime, $stat:literal, $thread_lifetime:lifetime) => {
        impl<$env_lifetime, T: 'static + Send> Scope<$env_lifetime, T, $stat> {
            /// The returned handle should only ever be await'ed on inside of the task scope this spawn is associated with, or any sub-task scopes (Otherwise the await will never complete in the Error case)
            fn spawn_with_custom_error_handling<
                R,
                V: 'static + Send,
                F: $thread_lifetime + Future<Output = R> + Send,
                ErrorHandler: $thread_lifetime + FnOnce(R) -> (T, Option<V>) + Send,
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
            /// Spawns a thread and gives you a handle to receive the Result::Ok of the thread by await on the returned handle (If this thread errors/panics the scope will exit with the error/panic)
            pub fn spawn_with_handle<
                V: 'static + Send,
                F: $thread_lifetime + Future<Output = anyhow::Result<V>> + Send,
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

            /// Spawns a thread and gives you a handle to receive the Result of the thread by awaiting on the returned handle (If this thread panics the scope will exit with the panic)
            pub fn spawn_without_error_handling<
                V: 'static + Send,
                F: $thread_lifetime + Future<Output = anyhow::Result<V>> + Send,
            >(
                &self,
                f: F,
            ) -> ScopedJoinHandle<anyhow::Result<V>> {
                self.spawn_with_custom_error_handling(|t| (Ok(()), Some(t)), f)
            }
        }

        impl<$env_lifetime> Scope<$env_lifetime, (), $stat> {
            // Spawns a thread and gives you a handle to wait for the thread to exit by await on the returned handle (If this thread errors/panics the scope will exit with the error/panic)
            pub fn spawn_with_handle<
                V: 'static + Send,
                F: $thread_lifetime + Future<Output = V> + Send,
            >(
                &self,
                f: F,
            ) -> ScopedJoinHandle<V> {
                self.spawn_with_custom_error_handling(|t| ((), Some(t)), f)
            }
        }
    };
}

/// This function guarantees all threads spawned using its scope object will finish before this function exits.
/// Thereby making accessing data outside of this scope from inside this scope via a reference is safe.
/// Which is why the closures/futures provided to Scope::spawn don't need static lifetimes.
#[tokio::main]
pub async fn with_main_task_scope<
    'a,
    C: for<'b> FnOnce(
        &'b Scope<'a, anyhow::Result<()>, false>,
    ) -> futures::future::BoxFuture<'b, anyhow::Result<T>>,
    T,
>(
    c: C,
) -> anyhow::Result<T> {
    // Safe as the provided future (via closure) is never cancelled
    unsafe { inner_with_task_scope(c).await }
}

impl<'a, T: Send + 'static> Scope<'a, T, false> {
    /// Spawn a thread that is guaranteed to exit or cancel/abort before the associated scope exits
    pub fn spawn<F: 'a + Future<Output = T> + Send>(&self, f: F) {
        // If this result is an error (I.e. the channel receiver was dropped then the stream was closed, and so we want to drop the handle to cancel the thread we just spawned)
        // This can only relevant for non-main scopes, because this can ony happen if the tokio::runtime is dropped early or the future the scope is in is dropped while running (i.e. via a timeout).
        let _result = self.sender.send(CancellingJoinHandle::new({
            let future: Pin<Box<dyn 'a + Future<Output = T> + Send>> = Box::pin(f);
            let future: Pin<Box<dyn 'static + Future<Output = T> + Send>> =
                unsafe { std::mem::transmute(future) };
            self.spawner.spawn(future)
        }));
    }
}

impl_spawn_ops!('a, false, 'a);

pub async fn with_task_scope<
    'a,
    C: for<'b> FnOnce(
        &'b Scope<'a, anyhow::Result<()>, true>,
    ) -> futures::future::BoxFuture<'b, anyhow::Result<T>>,
    T,
>(
    c: C,
) -> anyhow::Result<T> {
    // Safe as closures/futures are forced to have static lifetimes
    unsafe { inner_with_task_scope(c).await }
}

impl<'a, T: Send + 'static> Scope<'a, T, true> {
    /// Spawn a thread that is guaranteed to exit or cancel/abort before the associated scope exits
    pub fn spawn<F: 'static + Future<Output = T> + Send>(&self, f: F) {
        // If this result is an error (I.e. the channel receiver was dropped then the stream was closed, and so we want to drop the handle to cancel the thread we just spawned)
        // This can only relevant for non-main scopes, because this can ony happen if the tokio::runtime is dropped early or the future the scope is in is dropped while running (i.e. via a timeout).
        let _result = self
            .sender
            .send(CancellingJoinHandle::new(self.spawner.spawn(Box::pin(f))));
    }
}

impl_spawn_ops!('a, true, 'static);

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::*;

    #[test]
    fn main_task_scope_will_always_wait_for_all_scoped_threads() {
        for _i in 0..100 {
            const COUNT: u32 = 10;

            let a = std::sync::atomic::AtomicU32::new(0);

            let thread_start_count = std::sync::atomic::AtomicU32::new(0);

            let _result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> anyhow::Result<()> {
                    with_main_task_scope(|scope| {
                        async {
                            //scope.spawn(async { panic!() });
                            for _i in 0..COUNT {
                                scope.spawn(async {
                                    thread_start_count.fetch_add(1, Ordering::Relaxed);
                                    std::thread::sleep(std::time::Duration::from_millis(10));
                                    a.fetch_add(1, Ordering::Relaxed);
                                    Ok(())
                                });
                            }
                            while 10 != thread_start_count.load(Ordering::Relaxed) {
                                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                            }
                            panic!();
                        }
                        .boxed()
                    })
                }));

            assert_eq!(thread_start_count.load(Ordering::Relaxed), COUNT);
            assert_eq!(a.load(Ordering::Relaxed), COUNT);
        }
    }
}
