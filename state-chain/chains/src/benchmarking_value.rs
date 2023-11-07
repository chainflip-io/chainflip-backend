#[cfg(feature = "runtime-benchmarks")]
use cf_primitives::{
	chains::assets::{btc, dot, eth},
	Asset,
};

#[cfg(feature = "runtime-benchmarks")]
use ethereum_types::{H160, U256};

#[cfg(feature = "runtime-benchmarks")]
use crate::address::EncodedAddress;
#[cfg(feature = "runtime-benchmarks")]
use crate::address::ForeignChainAddress;
#[cfg(feature = "runtime-benchmarks")]
use crate::evm::{EvmFetchId, EvmTransactionMetadata};

/// Ensure type specifies a value to be used for benchmarking purposes.
pub trait BenchmarkValue {
	/// Returns a value suitable for running against benchmarks.
	#[cfg(feature = "runtime-benchmarks")]
	fn benchmark_value() -> Self;
}

/// Optional trait used to generage different benchmarking values.
pub trait BenchmarkValueExtended {
	/// Returns different values used for benchmarkings.
	#[cfg(feature = "runtime-benchmarks")]
	fn benchmark_value_by_id(id: u8) -> Self;
}

#[cfg(not(feature = "runtime-benchmarks"))]
impl<T> BenchmarkValue for T {}

#[cfg(not(feature = "runtime-benchmarks"))]
impl<T> BenchmarkValueExtended for T {}

#[macro_export]
macro_rules! impl_default_benchmark_value {
	($element:ty) => {
		#[cfg(feature = "runtime-benchmarks")]
		impl BenchmarkValue for $element {
			fn benchmark_value() -> Self {
				<$element>::default()
			}
		}
	};
}

#[cfg(feature = "runtime-benchmarks")]
impl<A: BenchmarkValue, B: BenchmarkValue> BenchmarkValue for (A, B) {
	fn benchmark_value() -> Self {
		(A::benchmark_value(), B::benchmark_value())
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValue for Asset {
	fn benchmark_value() -> Self {
		Self::Eth
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValue for eth::Asset {
	fn benchmark_value() -> Self {
		eth::Asset::Eth
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValue for dot::Asset {
	fn benchmark_value() -> Self {
		dot::Asset::Dot
	}
}

// TODO: Look at deduplicating this by including it in the macro
#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValue for btc::Asset {
	fn benchmark_value() -> Self {
		btc::Asset::Btc
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValue for ForeignChainAddress {
	fn benchmark_value() -> Self {
		ForeignChainAddress::Eth(Default::default())
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValueExtended for ForeignChainAddress {
	fn benchmark_value_by_id(id: u8) -> Self {
		ForeignChainAddress::Eth([id; 20].into())
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValue for EncodedAddress {
	fn benchmark_value() -> Self {
		EncodedAddress::Eth(Default::default())
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValue for EvmFetchId {
	fn benchmark_value() -> Self {
		Self::DeployAndFetch(1)
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValueExtended for EvmFetchId {
	fn benchmark_value_by_id(id: u8) -> Self {
		Self::DeployAndFetch(id as u64)
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValueExtended for () {
	fn benchmark_value_by_id(_id: u8) -> Self {
		Default::default()
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValue for EvmTransactionMetadata {
	fn benchmark_value() -> Self {
		Self {
			contract: H160::zero(),
			max_fee_per_gas: Some(U256::zero()),
			max_priority_fee_per_gas: Some(U256::zero()),
			gas_limit: None,
		}
	}
}

impl_default_benchmark_value!(());
impl_default_benchmark_value!(u32);
impl_default_benchmark_value!(u64);

macro_rules! impl_benchmark_value_extended_for_integer {
	( $i:ty ) => {
		#[cfg(feature = "runtime-benchmarks")]
		impl BenchmarkValueExtended for $i {
			fn benchmark_value_by_id(id: u8) -> Self {
				id as Self
			}
		}
	};
}

impl_benchmark_value_extended_for_integer!(u8);
impl_benchmark_value_extended_for_integer!(u16);
impl_benchmark_value_extended_for_integer!(u32);
impl_benchmark_value_extended_for_integer!(u64);
impl_benchmark_value_extended_for_integer!(u128);
