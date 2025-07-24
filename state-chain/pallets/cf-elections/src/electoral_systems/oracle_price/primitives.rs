use core::ops::{Mul, RangeInclusive, Sub};
use sp_std::ops::Add;

use crate::{
	electoral_systems::{oracle_price::price::Fraction, state_machine::common_imports::*},
	generic_tools::*,
};

#[cfg(test)]
use proptest_derive::Arbitrary;

def_derive! {
	#[cfg_attr(test, derive(Arbitrary))]
	#[derive(TypeInfo, PartialOrd, Ord, Default, Copy)]
	pub struct UnixTime{ pub seconds: u64 }
}

impl Add<Seconds> for UnixTime {
	type Output = UnixTime;

	fn add(self, rhs: Seconds) -> Self::Output {
		UnixTime { seconds: self.seconds.saturating_add(rhs.0) }
	}
}

impl Sub<Seconds> for UnixTime {
	type Output = UnixTime;

	fn sub(self, rhs: Seconds) -> Self::Output {
		UnixTime { seconds: self.seconds.saturating_sub(rhs.0) }
	}
}

def_derive! {
	#[cfg_attr(test, derive(Arbitrary))]
	#[derive(TypeInfo, Copy, Default)]
	pub struct Seconds(pub u64);
}

impl Mul<u64> for Seconds {
	type Output = Seconds;

	fn mul(self, rhs: u64) -> Self::Output {
		Seconds(self.0 * rhs)
	}
}

def_derive! {
	#[derive(TypeInfo, Copy, Default)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct BasisPoints(pub u16);
}

impl BasisPoints {
	pub fn to_fraction(self) -> Fraction<9999> {
		Fraction(self.0.into())
	}
}

pub trait AggregationValue = Ord + CommonTraits + MaybeArbitrary + 'static;

def_derive! {
	#[cfg_attr(test, derive(Arbitrary))]
	#[derive(TypeInfo)]
	pub struct Aggregated<A: AggregationValue> {
		pub median: A,
		pub iq_range: RangeInclusive<A>,
	}
}

impl<A: AggregationValue + Default> Default for Aggregated<A> {
	fn default() -> Self {
		Self { median: Default::default(), iq_range: Default::default()..=Default::default() }
	}
}

impl<A: AggregationValue> Aggregated<A> {
	pub fn from_single_value(value: A) -> Self {
		Self { median: value.clone(), iq_range: value.clone()..=value }
	}
}

pub fn compute_median<A: Ord + Clone>(mut values: Vec<A>) -> Option<A> {
	if values.is_empty() {
		return None;
	}
	let half = (values.len() - 1) / 2;
	let (_first_half, median, _second_half) = values.select_nth_unstable(half);
	Some(median.clone())
}

/// A safe version of `select_nth_unstable` that doesn't panic but returns None in case of failure.
pub fn select_nth_unstable_checked<A: Ord>(
	values: &mut [A],
	index: usize,
) -> Option<(&mut [A], &mut A, &mut [A])> {
	// `select_nth_unstable` panics if the index doesn't exist
	if index >= values.len() {
		return None;
	}
	Some(values.select_nth_unstable(index))
}

pub fn compute_aggregated<A: AggregationValue>(mut values: Vec<A>) -> Option<Aggregated<A>> {
	let quarter = values.len().saturating_sub(1) / 4;
	let half = (values.len().saturating_sub(1)) / 2;
	let (first_half, median, second_half) = select_nth_unstable_checked(&mut values, half)?;

	let first_quartile = select_nth_unstable_checked(first_half, quarter)
		.map(|res| res.1.clone())
		.unwrap_or(median.clone());
	let third_quartile = select_nth_unstable_checked(second_half, quarter)
		.map(|res| res.1.clone())
		.unwrap_or(median.clone());

	Some(Aggregated { median: median.clone(), iq_range: first_quartile..=third_quartile })
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::collection::vec;

	proptest! {
		#[test]
		fn fuzzy_compute_aggregated(votes in vec(any::<u16>(), 0..30)) {
			let _ = compute_aggregated(votes);
		}
	}
}
