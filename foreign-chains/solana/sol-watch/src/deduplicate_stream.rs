use std::{
	collections::{HashSet, VecDeque},
	hash::Hash,
	pin::Pin,
	task::{Context, Poll},
};

use futures::Stream;

pub trait DeduplicateStreamExt: Stream + Sized {
	fn deduplicate<Value, ExtractValue, OnDuplicate>(
		self,
		backlog_size: usize,
		extract_value: ExtractValue,
		on_duplicate: OnDuplicate,
	) -> DeduplicateStream<Self, Value, ExtractValue, OnDuplicate>
	where
		Value: Clone + Eq + Hash,
		ExtractValue: Fn(&Self::Item) -> Option<Value>,
		OnDuplicate: FnMut(Value, Self::Item),
	{
		DeduplicateStream::new(self, backlog_size, extract_value, on_duplicate)
	}
}

#[derive(Debug, Clone)]
#[pin_project::pin_project]
pub struct DeduplicateStream<Stream, Value, ExtractValue, OnDuplicate> {
	#[pin]
	inner: Stream,
	backlog_size: usize,
	queue: VecDeque<Value>,
	set: HashSet<Value>,
	extract_value: ExtractValue,
	on_duplicate: OnDuplicate,
}

impl<S, V, X, D> DeduplicateStream<S, V, X, D> {
	pub fn new(inner: S, backlog_size: usize, extract_value: X, on_duplicate: D) -> Self {
		Self {
			inner,
			backlog_size,
			queue: VecDeque::with_capacity(backlog_size + 1),
			set: HashSet::with_capacity(backlog_size),
			extract_value,
			on_duplicate,
		}
	}
}

impl<S> DeduplicateStreamExt for S where S: Sized + Stream {}

impl<S, V, X, D> Stream for DeduplicateStream<S, V, X, D>
where
	S: Stream,
	V: Clone + Eq + Hash,
	X: Fn(&S::Item) -> Option<V>,
	D: FnMut(V, S::Item),
{
	type Item = S::Item;

	fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		let mut this = self.project();

		Poll::Ready(loop {
			match std::task::ready!(this.inner.as_mut().poll_next(cx)) {
				None => break None,
				Some(item) =>
					if let Some(value) = (this.extract_value)(&item) {
						if this.set.contains(&value) {
							(this.on_duplicate)(value, item);
							continue
						}

						this.queue.push_back(value.clone());
						this.set.insert(value);

						if this.queue.len() > *this.backlog_size {
							if let Some(to_forget) = this.queue.pop_front() {
								this.set.remove(&to_forget);
							}
						}

						break Some(item)
					} else {
						break Some(item)
					},
			}
		})
	}
}
