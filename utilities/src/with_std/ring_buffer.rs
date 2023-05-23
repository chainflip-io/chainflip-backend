use std::collections::VecDeque;

/// A fixed size buffer that drops the oldest item when at full capacity
pub struct RingBuffer<T> {
	inner: VecDeque<T>,
}

impl<T> RingBuffer<T> {
	pub fn new(max_capacity: usize) -> Self {
		Self { inner: VecDeque::with_capacity(max_capacity) }
	}

	pub fn push(&mut self, item: T) {
		if self.inner.len() == self.inner.capacity() {
			self.inner.pop_front();
		}
		self.inner.push_back(item);
	}

	pub fn iter(&self) -> impl Iterator<Item = &T> {
		self.inner.iter()
	}
}

#[cfg(test)]
#[test]
fn check_ring_buffer() {
	let mut rb = RingBuffer::new(3);

	// Behaves like a Vec until it reaches full capacity
	rb.push(1);
	rb.push(2);
	rb.push(3);
	assert_eq!(rb.iter().copied().collect::<Vec<_>>(), vec![1, 2, 3]);

	// When adding another item, it shifts items dropping the left-most item
	rb.push(4);
	assert_eq!(rb.iter().copied().collect::<Vec<_>>(), vec![2, 3, 4]);
}
