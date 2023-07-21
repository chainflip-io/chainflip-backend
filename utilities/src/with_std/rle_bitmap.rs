use core::{iter::Step, ops::RangeBounds};
use std::collections::BTreeMap;

use itertools::Itertools;
use num_traits::Bounded;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RleBitmap<T: Ord> {
	rle_bitmap: BTreeMap<T, bool>,
}
impl<T: Ord + Copy + Step + Bounded> RleBitmap<T> {
	pub fn new(value: bool) -> Self {
		Self { rle_bitmap: [(T::min_value(), value)].into_iter().collect() }
	}

	pub fn get(&self, t: &T) -> bool {
		*self.rle_bitmap.range(..=t).next_back().unwrap().1
	}

	pub fn set(&mut self, t: T, value: bool) {
		self.set_range(t..=t, value);
	}

	pub fn set_range<Range: RangeBounds<T>>(&mut self, range: Range, value: bool) {
		let (exclusive_start, inclusive_start) = match range.start_bound() {
			core::ops::Bound::Included(t) => (<T as Step>::backward_checked(*t, 1), *t),
			core::ops::Bound::Excluded(t) => (
				Some(*t),
				if let Some(t) = <T as Step>::forward_checked(*t, 1) { t } else { return },
			),
			core::ops::Bound::Unbounded => (None, T::min_value()),
		};

		let option_exclusive_end = match range.end_bound() {
			core::ops::Bound::Included(t) => {
				assert!(inclusive_start <= *t);
				<T as Step>::forward_checked(*t, 1)
			},
			core::ops::Bound::Excluded(t) => {
				assert!(inclusive_start < *t);
				Some(*t)
			},
			core::ops::Bound::Unbounded => None,
		};

		if option_exclusive_end != Some(inclusive_start) {
			let does_start_value_match = exclusive_start
				.map_or(false, |exclusive_start| self.get(&exclusive_start) == value);
			let does_end_value_match = option_exclusive_end
				.map_or(true, |exclusive_end| self.get(&exclusive_end) == value);

			if does_start_value_match {
				self.rle_bitmap.remove(&inclusive_start);
			} else {
				self.rle_bitmap.insert(inclusive_start, value);
			}

			if does_end_value_match {
				if let Some(exclusive_end) = option_exclusive_end {
					self.rle_bitmap.remove(&exclusive_end);
				}
			} else {
				let exclusive_end = option_exclusive_end.unwrap();
				self.rle_bitmap.insert(exclusive_end, !value);
			}

			let mut end_section = if let Some(exclusive_end) = option_exclusive_end {
				self.rle_bitmap.split_off(&exclusive_end)
			} else {
				Default::default()
			};

			if let Some(next) = <T as Step>::forward_checked(inclusive_start, 1) {
				self.rle_bitmap.split_off(&next);
			}

			self.rle_bitmap.append(&mut end_section);
		}
	}

	pub fn invert(&mut self) {
		for (_, value) in self.rle_bitmap.iter_mut() {
			*value = !*value;
		}
	}

	pub fn is_superset(&self, other: &Self) -> bool {
		let mut temp = self.clone();
		for (start, option_end) in other.iter_ranges(true) {
			temp.set_range(
				(
					std::ops::Bound::Included(start),
					match option_end {
						Some(end) => std::ops::Bound::Excluded(end),
						None => std::ops::Bound::Unbounded,
					},
				),
				true,
			)
		}
		temp == *self
	}

	fn iter_ranges(&self, value: bool) -> impl Iterator<Item = (T, Option<T>)> + '_ {
		self.rle_bitmap
			.iter()
			.tuple_windows()
			.map(|(x, y)| (x, Some(y)))
			.chain(std::iter::once((self.rle_bitmap.iter().last().unwrap(), None)))
			.filter(move |((_, range_value), _)| **range_value == value)
			.map(|((start, _), option_end)| (*start, option_end.map(|(end, _)| *end)))
	}

	pub fn iter(&self, value: bool) -> impl Iterator<Item = T> + '_ {
		self.iter_ranges(value).flat_map(|(start, option_end)| {
			itertools::unfold(Some(start), |option_t| {
				if let Some(t) = option_t {
					let next = *t;
					*option_t = <T as Step>::forward_checked(*t, 1);
					Some(next)
				} else {
					None
				}
			})
			.take_while(move |t| if let Some(end) = option_end { *t < end } else { true })
		})
	}
}

#[cfg(test)]
mod tests {
	use super::RleBitmap;

	#[test]
	fn basic_tests() {
		let mut bitmap = RleBitmap::<u32>::new(true);

		assert!(bitmap.iter(false).next().is_none());

		bitmap.set(10, false);

		assert!(Iterator::eq(bitmap.iter(false), [10],));

		bitmap.set(11, false);

		assert!(Iterator::eq(bitmap.iter(false), [10, 11],));

		bitmap.set(12, false);

		assert!(Iterator::eq(bitmap.iter(false), [10, 11, 12],));

		bitmap.set(9, false);

		assert!(Iterator::eq(bitmap.iter(false), [9, 10, 11, 12],));

		bitmap.set(15, false);

		assert!(Iterator::eq(bitmap.iter(false), [9, 10, 11, 12, 15],));

		bitmap.set(u32::MAX, false);

		assert!(Iterator::eq(bitmap.iter(false), [9, 10, 11, 12, 15, u32::MAX],));

		bitmap.set(0, false);

		assert!(Iterator::eq(bitmap.iter(false), [0, 9, 10, 11, 12, 15, u32::MAX],));

		bitmap.invert();

		assert!(Iterator::eq(bitmap.iter(true), [0, 9, 10, 11, 12, 15, u32::MAX],));

		bitmap.set(u32::MAX, false);

		assert!(Iterator::eq(bitmap.iter(true), [0, 9, 10, 11, 12, 15],));

		bitmap.set(1, true);

		assert!(Iterator::eq(bitmap.iter(true), [0, 1, 9, 10, 11, 12, 15],));

		bitmap.set(0, false);

		assert!(Iterator::eq(bitmap.iter(true), [1, 9, 10, 11, 12, 15],));
	}
}
