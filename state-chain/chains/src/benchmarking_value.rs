use cf_primitives::{
	chains::assets::{btc, dot, eth},
	Asset, ForeignChainAddress, KeyId,
};

use crate::eth::EthereumIngressId;

/// Ensure type specifies a value to be used for benchmarking purposes.
pub trait BenchmarkValue {
	/// Returns a value suitable for running against benchmarks.

	fn benchmark_value() -> Self;
}

#[cfg(not(feature = "runtime-benchmarks"))]
impl<T> BenchmarkValue for T {}

#[macro_export]
macro_rules! impl_default_benchmark_value {
	($element:ty) => {
		impl BenchmarkValue for $element {
			//
			fn benchmark_value() -> Self {
				<$element>::default()
			}
		}
	};
}

impl BenchmarkValue for KeyId {
	fn benchmark_value() -> Self {
		Self {
			epoch_index: 1,
			public_key_bytes: hex_literal::hex!("02f87a827a6980843b9aca00843b9aca0082520894cfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcf808e646f5f736f6d657468696e672829c080a0b796e0276d89b0e02634d2f0cd5820e4af4bc0fcb76ecfcc4a3842e90d4b1651a07ab40be70e801fcd1e33460bfe34f03b8f390911658d49e58b0356a77b9432c0").to_vec()
		}
	}
}

impl BenchmarkValue for Asset {
	fn benchmark_value() -> Self {
		Self::Eth
	}
}

impl BenchmarkValue for eth::Asset {
	fn benchmark_value() -> Self {
		eth::Asset::Eth
	}
}

impl BenchmarkValue for dot::Asset {
	fn benchmark_value() -> Self {
		dot::Asset::Dot
	}
}

// TODO: Look at deduplicating this by including it in the macro

impl BenchmarkValue for btc::Asset {
	fn benchmark_value() -> Self {
		btc::Asset::Btc
	}
}

impl BenchmarkValue for ForeignChainAddress {
	fn benchmark_value() -> Self {
		ForeignChainAddress::Eth(Default::default())
	}
}

impl BenchmarkValue for EthereumIngressId {
	fn benchmark_value() -> Self {
		Self::UnDeployed(1)
	}
}

impl BenchmarkValue for [u8; 32] {
	fn benchmark_value() -> Self {
		[1u8; 32]
	}
}

impl_default_benchmark_value!(());
impl_default_benchmark_value!(u32);
impl_default_benchmark_value!(u64);
