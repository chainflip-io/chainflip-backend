use std::{
    fmt::Display,
    ops::{Deref, DerefMut},
    path::Path,
    time::Duration,
};

use anyhow::Context;
use futures::Stream;
use itertools::Itertools;
use jsonrpc_core_client::RpcError;

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
    use std::{fs::File, io::Write, panic::catch_unwind, path::PathBuf};

    use crate::testing::assert_ok;

    use super::*;
    use tempdir::TempDir;

    fn with_file<C: FnOnce(PathBuf) -> () + std::panic::UnwindSafe>(text: &[u8], closure: C) {
        let dir = TempDir::new("tests").unwrap();
        let file_path = dir.path().join("foo.txt");
        let result = catch_unwind(|| {
            let mut f = File::create(&file_path).unwrap();
            f.write_all(text).unwrap();
            closure(file_path);
        });
        dir.close().unwrap();
        result.unwrap();
    }

    #[test]
    fn load_hex_file() {
        with_file(b"   \"\'\'\"0xhex\"\'  ", |file_path| {
            assert_eq!(
                assert_ok!(read_clean_and_decode_hex_str_file(
                    &file_path,
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
                    &file_path,
                    "TEST",
                    |str| Ok(str.to_string())
                )),
                "h\" \'ex".to_string()
            );
        });
    }
}

/// Makes a stream that outputs () approximately every duration
pub fn make_periodic_stream(duration: Duration) -> impl Stream<Item = ()> {
    Box::pin(futures::stream::unfold((), move |_| async move {
        Some((tokio::time::sleep(duration).await, ()))
    }))
}

pub fn format_iterator<'a, I: 'static + Display, It: 'a + IntoIterator<Item = &'a I>>(
    it: It,
) -> String {
    format!("{}", it.into_iter().format(", "))
}
