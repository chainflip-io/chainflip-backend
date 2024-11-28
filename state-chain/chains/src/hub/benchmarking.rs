#![cfg(feature = "runtime-benchmarks")]

use crate::benchmarking_value::BenchmarkValue;

use super::{
	api::{rotate_vault_proxy, AssethubApi},
	dot::PolkadotReplayProtection,
};

const SIGNATURE: [u8; 64] = [1u8; 64];
const ACCOUNT_ID_1: [u8; 32] = [2u8; 32];
const ACCOUNT_ID_2: [u8; 32] = [3u8; 32];
const NONCE: u32 = 5;
const ENCODED_EXTRINSIC: [u8; 100] = [3u8; 100];

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
