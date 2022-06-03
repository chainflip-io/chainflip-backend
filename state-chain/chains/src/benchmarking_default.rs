#![cfg_attr(not(feature = "std"), no_std)]

/// A trait for implementing a default instance of a type for use in benchmarking.
pub trait BenchmarkValue {
	/// Returns a default value suitable for running against benchmarks.
	#[cfg(feature = "runtime-benchmarks")]
	fn benchmark_value() -> Self;
}

#[cfg(not(feature = "runtime-benchmarks"))]
impl<T> BenchmarkValue for T {}

#[macro_export]
macro_rules! impl_benchmark_default_for {
	($element:ty) => {
		#[cfg(feature = "runtime-benchmarks")]
		impl BenchmarkValue for $element {
			#[cfg(feature = "runtime-benchmarks")]
			fn benchmark_value() -> Self {
				<$element>::default()
			}
		}
	};
}
