#![cfg(feature = "runtime-benchmarks")]

use super::{api::SolanaApi, SolAddress, SolHash, SolSignature, SolTrackedData, SolTransaction};

use crate::benchmarking_value::{BenchmarkValue, BenchmarkValueExtended};

impl BenchmarkValue for SolAddress {
	fn benchmark_value() -> Self {
		[1u8; 32].into()
	}
}

impl BenchmarkValueExtended for SolAddress {
	fn benchmark_value_by_id(id: u8) -> Self {
		[id; 32].into()
	}
}

impl BenchmarkValue for SolTrackedData {
	fn benchmark_value() -> Self {
		SolTrackedData {}
	}
}

impl BenchmarkValue for SolTransaction {
	fn benchmark_value() -> Self {
		SolTransaction {}
	}
}

impl BenchmarkValue for SolSignature {
	fn benchmark_value() -> Self {
		[4u8; 64].into()
	}
}

impl BenchmarkValue for SolHash {
	fn benchmark_value() -> Self {
		[5u8; 32].into()
	}
}

impl<E> BenchmarkValue for SolanaApi<E> {
	fn benchmark_value() -> Self {
		SolanaApi::SetAggKeyWithAggKey {
			maybe_old_key: Some([7u8; 32].into()),
			new_key: [8u8; 32].into(),
		}
	}
}
