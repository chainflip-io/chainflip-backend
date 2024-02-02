#![cfg(feature = "runtime-benchmarks")]

use crate::evm::api::EvmReplayProtection;
use cf_primitives::chains::assets::arb;

use crate::{
	benchmarking_value::BenchmarkValue,
	eth::{api::set_agg_key_with_agg_key::SetAggKeyWithAggKey, AggKey},
	evm::api::EthereumTransactionBuilder,
};

use super::{api::ArbitrumApi, ArbitrumTrackedData};

impl BenchmarkValue for ArbitrumTrackedData {
	fn benchmark_value() -> Self {
		Self { base_fee: 10_000_000_000 }
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
			EvmReplayProtection::default(),
			SetAggKeyWithAggKey::new(AggKey::from_pubkey_compressed([2u8; 33])),
		)
		.into()
	}
}
