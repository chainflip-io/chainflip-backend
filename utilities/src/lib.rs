#![cfg_attr(not(feature = "std"), no_std)]
#![feature(const_option)]
#![feature(step_trait)]
#![cfg_attr(any(feature = "test-utils", test), feature(closure_track_caller))]
#![feature(array_methods)]

#[cfg(feature = "std")]
mod with_std;
#[cfg(feature = "std")]
pub use with_std::*;

mod without_std;
pub use without_std::*;

#[cfg(any(feature = "test-utils", test))]
pub mod testing;

pub type Port = u16;

/// Simply unwraps the value. Advantage of this is to make it clear in tests
/// what we are testing
#[macro_export]
macro_rules! assert_ok {
	($result:expr) => {
		$result.unwrap()
	};
}

#[macro_export]
macro_rules! assert_err {
	($result:expr) => {
		$result.unwrap_err()
	};
}

/// Note that the resulting `threshold` is the maximum number
/// of parties *not* enough to generate a signature,
/// i.e. at least `t+1` parties are required.
/// This follows the notation in the multisig library that
/// we are using and in the corresponding literature.
///
/// For the *success* threshold, use [success_threshold_from_share_count].
pub const fn threshold_from_share_count(share_count: u32) -> u32 {
	if 0 == share_count {
		0
	} else {
		(share_count.checked_mul(2).unwrap() - 1) / 3
	}
}

/// Returns the number of parties required for a threshold signature
/// ceremony to *succeed*.
pub fn success_threshold_from_share_count(share_count: u32) -> u32 {
	threshold_from_share_count(share_count).checked_add(1).unwrap()
}

/// Returns the number of bad parties required for a threshold signature
/// ceremony to *fail*.
pub fn failure_threshold_from_share_count(share_count: u32) -> u32 {
	share_count - threshold_from_share_count(share_count)
}

#[test]
fn check_threshold_calculation() {
	assert_eq!(threshold_from_share_count(150), 99);
	assert_eq!(threshold_from_share_count(100), 66);
	assert_eq!(threshold_from_share_count(90), 59);
	assert_eq!(threshold_from_share_count(3), 1);
	assert_eq!(threshold_from_share_count(4), 2);

	assert_eq!(success_threshold_from_share_count(150), 100);
	assert_eq!(success_threshold_from_share_count(100), 67);
	assert_eq!(success_threshold_from_share_count(90), 60);
	assert_eq!(success_threshold_from_share_count(3), 2);
	assert_eq!(success_threshold_from_share_count(4), 3);

	assert_eq!(failure_threshold_from_share_count(150), 51);
	assert_eq!(failure_threshold_from_share_count(100), 34);
	assert_eq!(failure_threshold_from_share_count(90), 31);
	assert_eq!(failure_threshold_from_share_count(3), 2);
	assert_eq!(failure_threshold_from_share_count(4), 2);
}

use core::mem::MaybeUninit;

struct PartialArray<T, const N: usize> {
	initialized_length: usize,
	array: [MaybeUninit<T>; N],
}
impl<T, const N: usize> PartialArray<T, N> {
	fn new() -> Self {
		Self {
			initialized_length: 0,
			// See: https://doc.rust-lang.org/nomicon/unchecked-uninit.html
			array: unsafe { MaybeUninit::<[MaybeUninit<T>; N]>::uninit().assume_init() },
		}
	}

	fn initialize(&mut self, t: T) {
		assert!(self.initialized_length < N);
		// This doesn't cause the previous T element to be dropped as if it was initialized, as the
		// assigment of MaybeUninit<T>'s instead of T's
		self.array[self.initialized_length] = MaybeUninit::new(t);
		self.initialized_length += 1;
	}

	fn into_array(mut self) -> [T; N] {
		assert_eq!(N, self.initialized_length);
		assert_eq!(core::mem::size_of::<[T; N]>(), core::mem::size_of::<[MaybeUninit<T>; N]>());
		// Don't drop the copied elements when PartialArray is dropped
		self.initialized_length = 0;
		unsafe { core::mem::transmute_copy::<_, [T; N]>(&self.array) }
	}
}
impl<T, const N: usize> Drop for PartialArray<T, N> {
	fn drop(&mut self) {
		for i in 0..self.initialized_length {
			unsafe {
				self.array[i].assume_init_drop();
			}
		}
	}
}

pub trait ArrayCollect {
	type Item;

	fn collect_array<const L: usize>(self) -> [Self::Item; L];
}

impl<It: Iterator<Item = Item>, Item> ArrayCollect for It {
	type Item = It::Item;

	fn collect_array<const L: usize>(self) -> [Self::Item; L] {
		let mut partial_array = PartialArray::<Self::Item, L>::new();

		for item in self {
			partial_array.initialize(item);
		}

		partial_array.into_array()
	}
}

pub trait SliceToArray {
	type Item: Copy;

	fn as_array<const L: usize>(&self) -> [Self::Item; L];
}

impl<Item: Copy> SliceToArray for [Item] {
	type Item = Item;

	fn as_array<const L: usize>(&self) -> [Self::Item; L] {
		self.iter().copied().collect_array::<L>()
	}
}

/// Tests that `collect_array` doesn't drop any of the iterator's items. For example it is important
/// to not copy an item, and then drop the copied instance.
#[test]
fn test_collect_array_dropping() {
	use std::sync::atomic::{AtomicUsize, Ordering};

	let instance_count = AtomicUsize::new(0);

	struct InstanceCounter<'a> {
		instance_count: &'a AtomicUsize,
	}
	impl<'a> InstanceCounter<'a> {
		fn new(instance_count: &'a AtomicUsize) -> Self {
			instance_count.fetch_add(1, Ordering::Relaxed);
			Self { instance_count }
		}
	}
	impl<'a> Drop for InstanceCounter<'a> {
		fn drop(&mut self) {
			self.instance_count.fetch_sub(1, Ordering::Relaxed);
		}
	}

	const INSTANCE_COUNT: usize = 6;

	let instances = std::iter::repeat_with(|| InstanceCounter::new(&instance_count))
		.take(INSTANCE_COUNT)
		.collect_array::<INSTANCE_COUNT>();

	assert_eq!(instance_count.load(Ordering::Relaxed), INSTANCE_COUNT);

	drop(instances);

	assert_eq!(instance_count.load(Ordering::Relaxed), 0);
}

#[test]
fn test_collect_array() {
	let v = vec![1, 2, 3, 4];

	const SIZE: usize = 3;

	let a = v.into_iter().take(SIZE).collect_array::<SIZE>();

	assert_eq!(a, [1, 2, 3]);
}

#[test]
fn test_collect_array_panics_on_invalid_length() {
	let v = vec![1, 2, 3, 4];

	assert_panics!(v.into_iter().collect_array::<3>());
}

#[test]
fn test_as_array() {
	let a = [1, 2, 3, 4];

	assert_eq!(a[1..3].as_array::<2>(), [2, 3]);
}

#[test]
fn test_as_array_panics_on_invalid_length() {
	let a = [1, 2, 3, 4];

	assert_panics!(a[1..3].as_array::<5>());
	assert_panics!(a[1..3].as_array::<1>());
}

pub fn format_iterator<'a, It: 'a + IntoIterator>(it: It) -> itertools::Format<'a, It::IntoIter>
where
	It::Item: core::fmt::Display,
{
	use itertools::Itertools;
	it.into_iter().format(", ")
}

pub fn all_same<Item: PartialEq, It: IntoIterator<Item = Item>>(it: It) -> Option<Item> {
	let mut it = it.into_iter();
	let option_item = it.next();
	match option_item {
		Some(item) =>
			if it.all(|other_items| other_items == item) {
				Some(item)
			} else {
				None
			},
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

#[cfg(test)]
mod test_asserts {
	use crate::assert_panics;

	#[test]
	fn test_assert_ok_unwrap_ok() {
		fn works() -> Result<i32, i32> {
			Ok(1)
		}
		let result = assert_ok!(works());
		assert_eq!(result, 1);
	}

	#[test]
	fn test_assert_ok_err() {
		assert_panics!(assert_ok!(Err::<u32, u32>(1)));
	}
}
