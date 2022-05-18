#![cfg_attr(not(feature = "std"), no_std)]

/// A trait for implementing a default instance of a type for use in benchmarking.
pub trait BenchmarkDefault {
	/// Returns a default value suitable for running against benchmarks.
	#[cfg(feature = "runtime-benchmarks")]
	fn benchmark_default() -> Self;
}

// #[cfg(feature = "runtime-benchmarks")]
// impl<T> BenchmarkDefault for T
// where
// 	T: sp_std::default::Default,
// {
// 	fn benchmark_default() -> Self {
// 		T::default()
// 	}
// }

#[cfg(not(feature = "runtime-benchmarks"))]
impl<T> BenchmarkDefault for T {}

#[macro_export]
macro_rules! impl_benchmark_default_for {
	($element:ty) => {
		#[cfg(feature = "runtime-benchmarks")]
		impl BenchmarkDefault for $element {
			#[cfg(feature = "runtime-benchmarks")]
			fn benchmark_default() -> Self {
				<$element>::default()
			}
		}
	};
}
