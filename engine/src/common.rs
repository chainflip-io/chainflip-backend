use std::{
    fmt::Display,
    ops::{Deref, DerefMut},
    path::Path,
};

use anyhow::Context;
use futures::{Future, TryStream};
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

/// This mutex implementation will panic when it is locked iff a thread previously panicked while holding it.
/// This ensures potentially broken data cannot be seen by other threads.
pub struct Mutex<T> {
    mutex: tokio::sync::Mutex<MutexStateAndPoisonFlag<T>>,
}
impl<T> Mutex<T> {
    pub fn new(t: T) -> Self {
        Self {
            mutex: tokio::sync::Mutex::new(MutexStateAndPoisonFlag {
                poisoned: false,
                state: t,
            }),
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
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    #[should_panic]
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
        mutex.lock().await;
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

pub fn read_clean_and_decode_hex_str_file<V, T: FnOnce(&str) -> Result<V, anyhow::Error>>(
    file: &Path,
    context: &str,
    t: T,
) -> Result<V, anyhow::Error> {
    std::fs::read_to_string(&file)
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
                assert_ok!(read_clean_and_decode_hex_str_file(
                    file_path,
                    "TEST",
                    |str| Ok(str.to_string())
                )),
                "hex".to_string()
            );
        });
    }

    #[test]
    fn load_invalid_hex_file() {
        with_file(b"   h\" \'ex  ", |file_path| {
            assert_eq!(
                assert_ok!(read_clean_and_decode_hex_str_file(
                    file_path,
                    "TEST",
                    |str| Ok(str.to_string())
                )),
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
        Some(item) => {
            if it.all(|other_items| other_items == item) {
                Some(item)
            } else {
                None
            }
        }
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
    fn try_map_and_end_after_error<Ok, F, Fut>(
        self,
        f: F,
    ) -> try_map_and_end_after_error::TryMapAndEndAfterError<Self, F, Fut>
    where
        F: FnMut(Self::Ok) -> Fut,
        Fut: Future<Output = Result<Ok, Self::Error>>,
    {
        try_map_and_end_after_error::TryMapAndEndAfterError::new(self, f)
    }

    fn try_map_with_state_and_end_after_error<S, F, Fut, Ok>(
        self,
        initial_state: S,
        f: F,
    ) -> try_map_with_state_and_end_after_error::TryMapWithStateAndEndAfterError<Self, S, F, Fut>
    where
        F: FnMut(S, Self::Ok) -> Fut,
        Fut: Future<Output = Result<(S, Ok), Self::Error>>,
    {
        try_map_with_state_and_end_after_error::TryMapWithStateAndEndAfterError::new(
            self,
            initial_state,
            f,
        )
    }
}
impl<S: TryStream + Sized> EngineTryStreamExt for S {}

mod try_map_with_state_and_end_after_error {
    use futures::{Future, Stream, TryStream};
    use futures_core::FusedStream;
    use pin_project::pin_project;
    use std::{fmt, pin::Pin, task::Poll};

    /// Stream for the [`try_map_with_state_and_end_after_error`](super::EngineTryStreamExt::try_map_with_state_and_end_after_error) method.
    #[must_use = "streams do nothing unless polled"]
    #[pin_project]
    pub struct TryMapWithStateAndEndAfterError<St: TryStream, S, F, Fut> {
        #[pin]
        stream: St,
        state: Option<S>,
        f: F,
        #[pin]
        future: Option<Fut>,
    }

    impl<St, S, F, Fut> fmt::Debug for TryMapWithStateAndEndAfterError<St, S, F, Fut>
    where
        St: TryStream + fmt::Debug,
        S: fmt::Debug,
        Fut: fmt::Debug,
    {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("Scan")
                .field("stream", &self.stream)
                .field("state", &self.state)
                .field("future", &self.future)
                .finish()
        }
    }

    impl<St, S, F, Fut, Ok> TryMapWithStateAndEndAfterError<St, S, F, Fut>
    where
        St: TryStream,
        F: FnMut(S, St::Ok) -> Fut,
        Fut: Future<Output = Result<(S, Ok), St::Error>>,
    {
        pub(super) fn new(stream: St, initial_state: S, f: F) -> Self {
            Self {
                stream,
                state: Some(initial_state),
                f,
                future: None,
            }
        }
    }

    impl<St, S, F, Fut, Ok> Stream for TryMapWithStateAndEndAfterError<St, S, F, Fut>
    where
        St: TryStream,
        F: FnMut(S, St::Ok) -> Fut,
        Fut: Future<Output = Result<(S, Ok), St::Error>>,
    {
        type Item = Result<Ok, St::Error>;

        fn poll_next(
            self: Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> Poll<Option<Self::Item>> {
            let mut this = self.project();

            let future = if let Some(future) = this.future.as_mut().as_pin_mut() {
                future
            } else if this.state.is_some() {
                if let Some(result) = futures::ready!(this.stream.as_mut().try_poll_next(cx)) {
                    let state = this.state.take().unwrap();
                    match result {
                        Ok(ok) => {
                            this.future.set(Some((this.f)(state, ok)));
                            this.future.as_mut().as_pin_mut().unwrap()
                        }
                        Err(error) => {
                            return Poll::Ready(Some(Err(error)));
                        }
                    }
                } else {
                    *this.state = None;
                    return Poll::Ready(None);
                }
            } else {
                *this.state = None;
                return Poll::Ready(None);
            };

            assert!(this.state.is_none());

            let result = futures::ready!(future.poll(cx));
            this.future.set(None);

            Poll::Ready(Some(match result {
                Ok((new_state, ok)) => {
                    *this.state = Some(new_state);
                    Ok(ok)
                }
                Err(error) => Err(error),
            }))
        }

        fn size_hint(&self) -> (usize, Option<usize>) {
            if self.state.is_none() && self.future.is_none() {
                (0, Some(0))
            } else {
                let (lower, upper) = self.stream.size_hint();
                (std::cmp::min(1, lower), upper) // can't know a lower bound, due to the predicate
            }
        }
    }

    impl<St, S, F, Fut, Ok> FusedStream for TryMapWithStateAndEndAfterError<St, S, F, Fut>
    where
        St: TryStream + FusedStream,
        F: FnMut(S, St::Ok) -> Fut,
        Fut: Future<Output = Result<(S, Ok), St::Error>>,
    {
        fn is_terminated(&self) -> bool {
            self.future.is_none() && (self.state.is_none() || self.stream.is_terminated())
        }
    }

    #[cfg(test)]
    mod tests {
        use futures::StreamExt;

        use crate::common::EngineTryStreamExt;

        #[tokio::test]
        async fn end_after_error_from_underlying_stream() {
            assert_eq!(
                vec![Ok(1), Err(2)],
                futures::stream::iter([Ok(1), Err(2), Ok(3)])
                    .try_map_with_state_and_end_after_error((), |(), n| async move { Ok(((), n)) })
                    .collect::<Vec<_>>()
                    .await
            );
        }

        #[tokio::test]
        async fn end_after_error_from_predicate() {
            assert_eq!(
                vec![Ok(1), Err(2)],
                futures::stream::iter([Ok(1), Ok(2), Ok(3)])
                    .try_map_with_state_and_end_after_error((), |(), n| async move {
                        if n == 2 {
                            Err(2)
                        } else {
                            Ok(((), n))
                        }
                    })
                    .collect::<Vec<_>>()
                    .await
            );
        }

        #[tokio::test]
        async fn state_is_updated() {
            assert_eq!(
                vec![Result::<u32, u32>::Ok(0), Ok(1), Ok(3)],
                futures::stream::iter([Ok(1), Ok(2), Ok(3)])
                    .try_map_with_state_and_end_after_error(0, |n, m| async move { Ok((n + m, n)) })
                    .collect::<Vec<_>>()
                    .await
            );
        }
    }
}

mod try_map_and_end_after_error {
    use futures::{Future, Stream, TryStream};
    use futures_core::FusedStream;
    use pin_project::pin_project;
    use std::{fmt, pin::Pin, task::Poll};

    /// Stream for the [`try_map_and_end_after_error`](super::EngineTryStreamExt::try_map_and_end_after_error) method.
    #[must_use = "streams do nothing unless polled"]
    #[pin_project]
    pub struct TryMapAndEndAfterError<St: TryStream, F, Fut> {
        #[pin]
        stream: St,
        done_taking: bool,
        f: F,
        #[pin]
        future: Option<Fut>,
    }

    impl<St, Fut, F> fmt::Debug for TryMapAndEndAfterError<St, F, Fut>
    where
        St: TryStream + fmt::Debug,
        Fut: fmt::Debug,
    {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("TryMapAndEndAfterError")
                .field("stream", &self.stream)
                .field("future", &self.future)
                .field("done_taking", &self.done_taking)
                .finish()
        }
    }

    impl<St, F, Fut, Ok> TryMapAndEndAfterError<St, F, Fut>
    where
        St: TryStream,
        F: FnMut(St::Ok) -> Fut,
        Fut: Future<Output = Result<Ok, St::Error>>,
    {
        pub(super) fn new(stream: St, f: F) -> Self {
            Self {
                stream,
                done_taking: false,
                f,
                future: None,
            }
        }
    }

    impl<St, F, Fut, Ok> Stream for TryMapAndEndAfterError<St, F, Fut>
    where
        St: TryStream,
        F: FnMut(St::Ok) -> Fut,
        Fut: Future<Output = Result<Ok, St::Error>>,
    {
        type Item = Result<Ok, St::Error>;

        fn poll_next(
            self: Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> Poll<Option<Self::Item>> {
            let mut this = self.project();

            let future = if let Some(future) = this.future.as_mut().as_pin_mut() {
                future
            } else if *this.done_taking {
                return Poll::Ready(None);
            } else if let Some(result) = futures::ready!(this.stream.as_mut().try_poll_next(cx)) {
                match result {
                    Ok(ok) => {
                        this.future.set(Some((this.f)(ok)));
                        this.future.as_mut().as_pin_mut().unwrap()
                    }
                    Err(error) => {
                        *this.done_taking = true;
                        return Poll::Ready(Some(Err(error)));
                    }
                }
            } else {
                *this.done_taking = true;
                return Poll::Ready(None);
            };

            assert!(!*this.done_taking);

            let result = futures::ready!(future.poll(cx));
            this.future.set(None);

            Poll::Ready(Some(match result {
                Ok(ok) => Ok(ok),
                Err(error) => {
                    *this.done_taking = true;
                    Err(error)
                }
            }))
        }

        fn size_hint(&self) -> (usize, Option<usize>) {
            if self.done_taking {
                (0, Some(0))
            } else {
                let (lower, upper) = self.stream.size_hint();
                (std::cmp::min(1, lower), upper) // can't know a lower bound, due to the predicate
            }
        }
    }

    impl<St, F, Fut, Ok> FusedStream for TryMapAndEndAfterError<St, F, Fut>
    where
        St: TryStream + FusedStream,
        F: FnMut(St::Ok) -> Fut,
        Fut: Future<Output = Result<Ok, St::Error>>,
    {
        fn is_terminated(&self) -> bool {
            self.done_taking || (self.future.is_none() && self.stream.is_terminated())
        }
    }

    #[cfg(test)]
    mod tests {
        use futures::StreamExt;

        use crate::common::EngineTryStreamExt;

        #[tokio::test]
        async fn end_after_error_from_underlying_stream() {
            assert_eq!(
                vec![Ok(1), Err(2)],
                futures::stream::iter([Ok(1), Err(2), Ok(3)])
                    .try_map_and_end_after_error(|n| async move { Ok(n) })
                    .collect::<Vec<_>>()
                    .await
            );
        }

        #[tokio::test]
        async fn end_after_error_from_predicate() {
            assert_eq!(
                vec![Ok(1), Err(2)],
                futures::stream::iter([Ok(1), Ok(2), Ok(3)])
                    .try_map_and_end_after_error(|n| async move {
                        if n == 2 {
                            Err(2)
                        } else {
                            Ok(n)
                        }
                    })
                    .collect::<Vec<_>>()
                    .await
            );
        }
    }
}
