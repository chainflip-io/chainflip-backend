use futures::{Future, Stream};
use std::{collections::BTreeSet, iter::IntoIterator};

#[pin_project::pin_project]
struct Wrapper<Key, Fut> {
	key: Key,
	#[pin]
	future: Fut,
}
impl<Key: Copy, Fut: Future> Future for Wrapper<Key, Fut> {
	type Output = (Key, Fut::Output);

	fn poll(
		self: core::pin::Pin<&mut Self>,
		cx: &mut core::task::Context<'_>,
	) -> core::task::Poll<Self::Output> {
		let this = self.project();
		this.future.poll(cx).map(|output| (*this.key, output))
	}
}

#[pin_project::pin_project]
pub struct FutureMap<Key, Fut> {
	#[pin]
	futures: futures::stream::FuturesUnordered<Wrapper<Key, Fut>>,
	keys: BTreeSet<Key>,
}
impl<Key, Fut> Default for FutureMap<Key, Fut> {
	fn default() -> Self {
		Self { futures: Default::default(), keys: Default::default() }
	}
}
impl<Key: Ord + Copy, Fut: Future + Unpin> FutureMap<Key, Fut> {
	pub fn insert(&mut self, key: Key, future: Fut) {
		self.remove(key);
		self.keys.insert(key);
		self.futures.push(Wrapper { key, future });
	}

	pub fn remove(&mut self, key: Key) -> Option<Fut> {
		if self.keys.contains(&key) {
			let mut cancelled_future = None;

			let futures = std::mem::take(&mut self.futures).into_iter();

			for future in futures {
				if future.key != key {
					self.futures.push(future);
				} else {
					assert!(cancelled_future.is_none());
					cancelled_future = Some(future.future);
				}
			}

			cancelled_future
		} else {
			None
		}
	}

	pub fn len(&self) -> usize {
		self.keys.len()
	}

	pub fn is_empty(&self) -> bool {
		self.keys.is_empty()
	}
}
impl<Key: Ord + Copy, Fut: Future + Unpin> Stream for FutureMap<Key, Fut> {
	type Item = (Key, Fut::Output);

	fn poll_next(
		self: core::pin::Pin<&mut Self>,
		cx: &mut core::task::Context<'_>,
	) -> core::task::Poll<Option<Self::Item>> {
		let this = self.project();
		this.futures.poll_next(cx).map(|option| {
			option.map(|(key, output)| {
				assert!(this.keys.remove(&key));
				(key, output)
			})
		})
	}
}
