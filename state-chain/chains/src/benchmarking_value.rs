use cf_primitives::chains::assets::{any, dot, eth};

/// Ensure type specifies a value to be used for benchmarking purposes.
pub trait BenchmarkValue {
	/// Returns a value suitable for running against benchmarks.
	#[cfg(feature = "runtime-benchmarks")]
	fn benchmark_value() -> Self;
}

#[cfg(not(feature = "runtime-benchmarks"))]
impl<T> BenchmarkValue for T {}

#[macro_export]
macro_rules! impl_default_benchmark_value {
	($element:ty) => {
		#[cfg(feature = "runtime-benchmarks")]
		impl BenchmarkValue for $element {
			// #[cfg(feature = "runtime-benchmarks")]
			fn benchmark_value() -> Self {
				<$element>::default()
			}
		}
	};
}

impl_default_benchmark_value!(());
impl_default_benchmark_value!(u64);
impl_default_benchmark_value!(any::Asset);
impl_default_benchmark_value!(eth::Asset);
impl_default_benchmark_value!(dot::Asset);
