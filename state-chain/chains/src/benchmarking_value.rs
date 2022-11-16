use cf_primitives::{
	chains::{
		assets::{dot, eth},
		AnyChainAccount,
	},
	Asset,
};

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

impl BenchmarkValue for Asset {
	#[cfg(feature = "runtime-benchmarks")]
	fn benchmark_value() -> Self {
		Self::Eth
	}
}

impl BenchmarkValue for eth::Asset {
	#[cfg(feature = "runtime-benchmarks")]
	fn benchmark_value() -> Self {
		eth::Asset::Eth
	}
}

impl BenchmarkValue for dot::Asset {
	#[cfg(feature = "runtime-benchmarks")]
	fn benchmark_value() -> Self {
		dot::Asset::Dot
	}
}

impl BenchmarkValue for AnyChainAccount {
	#[cfg(feature = "runtime-benchmarks")]
	fn benchmark_value() -> Self {
		[0u8; 32].into()
	}
}

impl_default_benchmark_value!(());
impl_default_benchmark_value!(u64);
