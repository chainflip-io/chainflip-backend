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
pub trait Aggregation {
	type Of<X: AggregationValue>: CommonTraits + MaybeArbitrary;
	fn canonical<X: AggregationValue>(price: &Self::Of<X>) -> X;
	fn compute<X: AggregationValue>(value: &[X]) -> Option<Self::Of<X>>;
	fn single<X: AggregationValue>(value: &X) -> Self::Of<X>;
}
pub type Apply<A, X> = <A as Aggregation>::Of<X>;

def_derive! {
	#[cfg_attr(test, derive(Arbitrary))]
	#[derive(TypeInfo)]
	pub struct AggregatedF;
}

impl Aggregation for AggregatedF {
	type Of<X: AggregationValue> = Aggregated<X>;

	fn canonical<X: AggregationValue>(value: &Self::Of<X>) -> X {
		value.median.clone()
	}

	fn compute<X: AggregationValue>(value: &[X]) -> Option<Self::Of<X>> {
		compute_aggregated(value.to_vec())
	}

	fn single<X: AggregationValue>(value: &X) -> Self::Of<X> {
		Aggregated::from_single_value(value.clone())
	}
}

def_derive! {
	#[cfg_attr(test, derive(Arbitrary))]
	#[derive(TypeInfo)]
	pub struct Aggregated<A: CommonTraits + MaybeArbitrary + PartialOrd> {
		pub median: A,
		pub iq_range: RangeInclusive<A>,
	}
}

impl<A: CommonTraits + MaybeArbitrary + PartialOrd> Aggregated<A> {
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

pub fn compute_aggregated<A: AggregationValue>(mut values: Vec<A>) -> Option<Aggregated<A>> {
	if values.is_empty() {
		return None;
	}

	let quarter = values.len() / 4;
	let half = (values.len() - 1) / 2;
	let (first_half, median, second_half) = values.select_nth_unstable(half);

	// TODO, these two might need to be double checked
	let first_quartile = if first_half.is_empty() {
		median.clone()
	} else {
		first_half.select_nth_unstable(quarter).1.clone()
	};
	let third_quartile = if second_half.is_empty() {
		median.clone()
	} else {
		second_half.select_nth_unstable(quarter).1.clone()
	};

	Some(Aggregated { median: median.clone(), iq_range: first_quartile..=third_quartile })
}
