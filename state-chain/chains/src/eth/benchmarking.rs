#![cfg(feature = "runtime-benchmarks")]

use crate::{
	benchmarking_value::BenchmarkValue,
	evm::api::{EthereumTransactionBuilder, EvmReplayProtection},
};

use super::{
	api::{update_flip_supply::UpdateFlipSupply, EthereumApi},
	EthereumTrackedData,
};

impl<E> BenchmarkValue for EthereumApi<E> {
	fn benchmark_value() -> Self {
		EthereumTransactionBuilder::new_unsigned(
			EvmReplayProtection::default(),
			UpdateFlipSupply::new(1000000u128, 1u64),
		)
		.into()
	}
}

impl BenchmarkValue for EthereumTrackedData {
	fn benchmark_value() -> Self {
		Self { base_fee: 10_000_000_000, priority_fee: 2_000_000_000 }
	}
}
