use std::{
    fmt::Display,
    iter::FromIterator,
    ops::{Deref, DerefMut},
    path::Path,
    time::Duration,
};

use anyhow::Context;
use itertools::Itertools;
use jsonrpc_core_client::RpcError;
use tokio::time::Instant;

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

// Needed due to the jsonrpc maintainer's not definitely unquestionable decision to impl their error types without the Sync trait
pub fn rpc_error_into_anyhow_error(error: RpcError) -> anyhow::Error {
    anyhow::Error::msg(format!("{}", error))
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
    use crate::testing::{assert_ok, with_file};

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

/// Makes a tick that outputs every duration and if ticks are "missed" (as tick() wasn't called for some time)
/// it will immediately output a single tick on the next call to tick() and resume ticking every duration
pub fn make_periodic_tick(duration: Duration) -> tokio::time::Interval {
    let mut interval = tokio::time::interval_at(Instant::now() + duration, duration);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    interval
}

#[cfg(test)]
mod tests_make_periodic_tick {
    use crate::testing::assert_ok;

    use super::*;

    #[tokio::test]
    async fn skips_ticks_test() {
        const PERIOD: f32 = 0.25;

        let mut tick = make_periodic_tick(Duration::from_secs_f32(PERIOD));

        // Skip two ticks
        tokio::time::sleep(Duration::from_secs_f32(PERIOD * 2.5)).await;

        // Next tick outputs immediately
        assert_ok!(tokio::time::timeout(Duration::from_secs_f32(0.01), tick.tick()).await);

        // We skip ticks instead of bursting ticks (Next tick should occur in PERIOD * 0.5)
        assert!(
            tokio::time::timeout(Duration::from_secs_f32(PERIOD * 0.25), tick.tick())
                .await
                .is_err()
        );

        // Ticks continue to be insync with duration (Next tick should occur in PERIOD * 0.25)
        assert_ok!(tokio::time::timeout(Duration::from_secs_f32(PERIOD * 0.35), tick.tick()).await);
    }

    #[tokio::test]
    async fn period_test() {
        const PERIOD: f32 = 0.25;

        let mut tick = make_periodic_tick(Duration::from_secs_f32(PERIOD));

        for _i in 0..4 {
            assert!(
                tokio::time::timeout(Duration::from_secs_f32(PERIOD * 0.8), tick.tick())
                    .await
                    .is_err()
            );
            tick.tick().await;
        }
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
