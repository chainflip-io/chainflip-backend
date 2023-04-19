use std::{
	ops::{Deref, DerefMut},
	time::Duration,
};

use futures::Future;

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
