#![cfg(feature = "runtime-benchmarks")]

use crate::{
	evm::api::{EvmEnvironmentProvider, EvmReplayProtection},
	ReplayProtectionProvider,
};
use cf_primitives::chains::{assets::arb, Arbitrum};

use crate::{
	benchmarking_value::BenchmarkValue,
	evm::{
		api::{set_agg_key_with_agg_key::SetAggKeyWithAggKey, EvmTransactionBuilder},
		AggKey,
	},
};

use super::{api::ArbitrumApi, ArbitrumTrackedData};

impl BenchmarkValue for ArbitrumTrackedData {
	fn benchmark_value() -> Self {
		Self { base_fee: 10_000_000_000, gas_limit_multiplier: 1.into() }
	}
}

impl BenchmarkValue for arb::Asset {
	fn benchmark_value() -> Self {
		arb::Asset::ArbEth
	}
}

impl<E: ReplayProtectionProvider<Arbitrum> + EvmEnvironmentProvider<Arbitrum>> BenchmarkValue
	for ArbitrumApi<E>
{
	fn benchmark_value() -> Self {
		EvmTransactionBuilder::new_unsigned(
			EvmReplayProtection::default(),
			SetAggKeyWithAggKey::new(AggKey::from_pubkey_compressed([2u8; 33])),
		)
		.into()
	}
}
