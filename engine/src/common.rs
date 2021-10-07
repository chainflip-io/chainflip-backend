use std::ops::{Deref, DerefMut};

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
