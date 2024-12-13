#![cfg(feature = "runtime-benchmarks")]

use super::{
	api::{SolanaApi, VaultSwapAccountAndSender},
	SolAddress, SolHash, SolMessage, SolSignature, SolTrackedData, SolTransaction,
	SolanaTransactionData,
};

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
		SolTrackedData { priority_fee: 100_000 }
	}
}

impl BenchmarkValue for SolMessage {
	fn benchmark_value() -> Self {
		Self::new_with_blockhash(&[], None, &SolHash::default().into())
	}
}

impl BenchmarkValue for SolTransaction {
	fn benchmark_value() -> Self {
		SolTransaction::new_unsigned(SolMessage::benchmark_value())
	}
}

impl BenchmarkValue for SolanaTransactionData {
	fn benchmark_value() -> Self {
		SolanaTransactionData {
			serialized_transaction: SolTransaction::benchmark_value()
				.finalize_and_serialize()
				.expect("Failed to serialize payload"),
			skip_preflight: false,
		}
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

impl<E: crate::sol::api::SolanaEnvironment> BenchmarkValue for SolanaApi<E> {
	fn benchmark_value() -> Self {
		SolanaApi::<E>::rotate_agg_key([8u8; 32].into())
			.expect("Benchmark value for SolApi must work.")
	}
}

impl BenchmarkValue for VaultSwapAccountAndSender {
	fn benchmark_value() -> Self {
		Self {
			swap_sender: BenchmarkValue::benchmark_value(),
			vault_swap_account: BenchmarkValue::benchmark_value(),
		}
	}
}
