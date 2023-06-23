#![cfg(feature = "runtime-benchmarks")]

use cf_primitives::chains::assets::arb;

use crate::{
	benchmarking_value::{BenchmarkValue, BenchmarkValueExtended},
	eth::{
		api::{
			set_agg_key_with_agg_key::SetAggKeyWithAggKey, EthereumReplayProtection,
			EthereumTransactionBuilder,
		},
		AggKey,
	},
};

use super::{api::ArbitrumApi, ArbitrumAddress, ArbitrumTrackedData};

impl BenchmarkValue for ArbitrumTrackedData {
	fn benchmark_value() -> Self {
		Self { base_fee: 10_000_000_000 }
	}
}

impl BenchmarkValue for ArbitrumAddress {
	fn benchmark_value() -> Self {
		ArbitrumAddress([1_u8; 20])
	}
}

impl BenchmarkValueExtended for ArbitrumAddress {
	fn benchmark_value_by_id(id: u8) -> Self {
		ArbitrumAddress([id; 20])
	}
}

impl BenchmarkValue for arb::Asset {
	fn benchmark_value() -> Self {
		arb::Asset::ArbEth
	}
}

impl<E> BenchmarkValue for ArbitrumApi<E> {
	fn benchmark_value() -> Self {
		EthereumTransactionBuilder::new_unsigned(
			EthereumReplayProtection::default(),
			SetAggKeyWithAggKey::new(AggKey::from_pubkey_compressed([2u8; 33])),
		)
		.into()
	}
}
