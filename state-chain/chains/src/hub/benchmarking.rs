#![cfg(feature = "runtime-benchmarks")]

use crate::benchmarking_value::BenchmarkValue;

use super::{
	api::{rotate_vault_proxy, AssethubApi},
	dot::{PolkadotReplayProtection, RuntimeVersion},
	AssethubTrackedData,
};

impl<E> BenchmarkValue for AssethubApi<E> {
	fn benchmark_value() -> Self {
		AssethubApi::RotateVaultProxy(rotate_vault_proxy::extrinsic_builder(
			PolkadotReplayProtection {
				genesis_hash: Default::default(),
				signer: BenchmarkValue::benchmark_value(),
				nonce: Default::default(),
			},
			Some(Default::default()),
			Default::default(),
			Default::default(),
		))
	}
}

impl BenchmarkValue for AssethubTrackedData {
	fn benchmark_value() -> Self {
		AssethubTrackedData {
			median_tip: 2,
			runtime_version: RuntimeVersion { spec_version: 17, transaction_version: 16 },
		}
	}
}
