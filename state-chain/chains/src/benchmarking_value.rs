#[cfg(feature = "runtime-benchmarks")]
use cf_primitives::{
	chains::assets::{any::AssetMap, arb, btc, dot, eth, sol},
	Asset,
};
#[cfg(feature = "runtime-benchmarks")]
use core::str::FromStr;

#[cfg(feature = "runtime-benchmarks")]
use ethereum_types::{H160, U256};

#[cfg(feature = "runtime-benchmarks")]
use sp_std::vec;

#[cfg(feature = "runtime-benchmarks")]
use crate::{
	address::{EncodedAddress, ForeignChainAddress},
	btc::{Utxo, UtxoId},
	dot::PolkadotTransactionId,
	evm::{DepositDetails, EvmFetchId, EvmTransactionMetadata},
};

/// Ensure type specifies a value to be used for benchmarking purposes.
pub trait BenchmarkValue {
	/// Returns a value suitable for running against benchmarks.
	#[cfg(feature = "runtime-benchmarks")]
	fn benchmark_value() -> Self;
}

/// Optional trait used to generate different benchmarking values.
pub trait BenchmarkValueExtended {
	/// Returns different values used for benchmarking.
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
impl BenchmarkValue for sol::Asset {
	fn benchmark_value() -> Self {
		sol::Asset::Sol
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
impl BenchmarkValue for crate::sol::SolanaDepositFetchId {
	fn benchmark_value() -> Self {
		crate::sol::SolanaDepositFetchId {
			channel_id: 923_601_931u64,
			address: crate::sol::SolAddress::from_str(
				"4Spd3kst7XsA9pdp5ArfdXxEK4xfW88eRKbyQBmMvwQj",
			)
			.unwrap(),
			bump: 255u8,
		}
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValueExtended for crate::sol::SolanaDepositFetchId {
	fn benchmark_value_by_id(id: u8) -> Self {
		crate::sol::SolanaDepositFetchId {
			channel_id: id as u64,
			address: crate::sol::SolAddress([id; 32]),
			bump: 255u8,
		}
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

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValue for PolkadotTransactionId {
	fn benchmark_value() -> Self {
		Self { block_number: 0u32, extrinsic_index: 0u32 }
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl<T: BenchmarkValue> BenchmarkValue for AssetMap<T> {
	fn benchmark_value() -> Self {
		Self::from_fn(|_| T::benchmark_value())
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl<T: BenchmarkValue> BenchmarkValue for eth::AssetMap<T> {
	fn benchmark_value() -> Self {
		Self::from_fn(|_| T::benchmark_value())
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl<T: BenchmarkValue> BenchmarkValue for btc::AssetMap<T> {
	fn benchmark_value() -> Self {
		Self::from_fn(|_| T::benchmark_value())
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl<T: BenchmarkValue> BenchmarkValue for dot::AssetMap<T> {
	fn benchmark_value() -> Self {
		Self::from_fn(|_| T::benchmark_value())
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl<T: BenchmarkValue> BenchmarkValue for arb::AssetMap<T> {
	fn benchmark_value() -> Self {
		Self::from_fn(|_| T::benchmark_value())
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl<T: BenchmarkValue> BenchmarkValue for sol::AssetMap<T> {
	fn benchmark_value() -> Self {
		Self::from_fn(|_| T::benchmark_value())
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValue for DepositDetails {
	fn benchmark_value() -> Self {
		use sp_core::H256;
		Self { tx_hashes: Some(vec![H256::default()]) }
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValue for Utxo {
	fn benchmark_value() -> Self {
		Utxo {
			id: UtxoId::benchmark_value(),
			amount: 10_000_000,
			deposit_address: crate::btc::deposit_address::DepositAddress::new([0; 32], 0),
		}
	}
}

impl_default_benchmark_value!(());
impl_default_benchmark_value!(u32);
impl_default_benchmark_value!(u64);

#[macro_export]
macro_rules! impl_tuple_benchmark_value {
	($($t:ident),* $(,)?) => {
		#[cfg(feature = "runtime-benchmarks")]
		impl<$($t: BenchmarkValue, )*> BenchmarkValue for ($($t,)*) {
			fn benchmark_value() -> Self {
				(
					$($t::benchmark_value(),)*
				)
			}
		}
	};
}

impl_tuple_benchmark_value!(A, B);
impl_tuple_benchmark_value!(A, B, C);
impl_tuple_benchmark_value!(A, B, C, D);
impl_tuple_benchmark_value!(A, B, C, D, EE);
impl_tuple_benchmark_value!(A, B, C, D, EE, F);
