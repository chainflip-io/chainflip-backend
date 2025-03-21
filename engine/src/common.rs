// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use cf_utilities::assert_matches;
use std::ops::{Deref, DerefMut};

struct MutexStateAndPoisonFlag<T> {
	poisoned: bool,
	state: T,
}

pub struct MutexGuard<'a, T> {
	guard: tokio::sync::MutexGuard<'a, MutexStateAndPoisonFlag<T>>,
}
impl<T> Deref for MutexGuard<'_, T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		&self.guard.deref().state
	}
}
impl<T> DerefMut for MutexGuard<'_, T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.guard.deref_mut().state
	}
}
impl<T> Drop for MutexGuard<'_, T> {
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
	use cf_utilities::assert_future_panics;

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

pub struct Signaller<T> {
	sender: async_broadcast::Sender<T>,
}
impl<T: Clone + Send + 'static> Signaller<T> {
	pub fn signal(self, t: T) {
		assert_matches!(
			self.sender.try_broadcast(t),
			Ok(None) | Err(async_broadcast::TrySendError::Closed(_))
		);
	}
}

#[derive(Clone)]
pub enum Signal<T> {
	Pending(async_broadcast::Receiver<T>),
	Signalled(T),
}
impl<T: Clone + Send + 'static> Signal<T> {
	pub fn new() -> (Signaller<T>, Self) {
		let (sender, receiver) = async_broadcast::broadcast(1);

		(Signaller { sender }, Self::Pending(receiver))
	}

	pub fn signalled(t: T) -> Self {
		Self::Signalled(t)
	}

	pub fn get(&mut self) -> Option<&T> {
		match self {
			Signal::Pending(receiver) => match receiver.try_recv() {
				Ok(t) => {
					*self = Self::Signalled(t);
					match self {
						Signal::Pending(_) => unreachable!(),
						Signal::Signalled(t) => Some(t),
					}
				},
				Err(_err) => None,
			},
			Signal::Signalled(t) => Some(t),
		}
	}

	pub async fn wait(self) -> T {
		match self {
			Signal::Pending(mut receiver) => match receiver.recv().await {
				Ok(t) => t,
				Err(_err) => futures::future::pending().await,
			},
			Signal::Signalled(t) => t,
		}
	}
}

pub fn option_inner<T, S>(option_tup: Option<(T, S)>) -> (Option<T>, Option<S>) {
	match option_tup {
		Some((t, s)) => (Some(t), Some(s)),
		None => (None, None),
	}
}
