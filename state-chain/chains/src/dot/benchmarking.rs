#![cfg(feature = "runtime-benchmarks")]

use crate::{
	benchmarking_value::{BenchmarkValue, BenchmarkValueExtended},
	dot::{
		BalancesCall, PolkadotAccountIdLookup, PolkadotChargeTransactionPayment,
		PolkadotCheckMortality, PolkadotCheckNonce, PolkadotRuntimeCall, PolkadotSignature,
		PolkadotSignedExtra, PolkadotTransactionData, PolkadotUncheckedExtrinsic,
	},
};

use sp_runtime::generic::Era;

use super::{
	api::{create_anonymous_vault, PolkadotApi},
	EncodedPolkadotPayload, PolkadotAccountId, PolkadotReplayProtection, PolkadotTrackedData, TxId,
};

const SIGNATURE: PolkadotSignature = PolkadotSignature::from_aliased([1u8; 64]);
const ACCOUNT_ID_1: PolkadotAccountId = PolkadotAccountId::from_aliased([2u8; 32]);
const ACCOUNT_ID_2: PolkadotAccountId = PolkadotAccountId::from_aliased([3u8; 32]);
const NONCE: u32 = 5;
const ENCODED_EXTRINSIC: [u8; 100] = [3u8; 100];

impl BenchmarkValue for PolkadotUncheckedExtrinsic {
	fn benchmark_value() -> Self {
		PolkadotUncheckedExtrinsic::new_signed(
			PolkadotRuntimeCall::Balances(BalancesCall::transfer_all {
				dest: PolkadotAccountIdLookup::from(ACCOUNT_ID_1),
				keep_alive: true,
			}),
			ACCOUNT_ID_2,
			SIGNATURE,
			PolkadotSignedExtra((
				(),
				(),
				(),
				(),
				PolkadotCheckMortality(Era::Immortal),
				PolkadotCheckNonce(NONCE),
				(),
				PolkadotChargeTransactionPayment(0),
				(),
			)),
		)
	}
}

impl BenchmarkValue for PolkadotSignature {
	fn benchmark_value() -> Self {
		SIGNATURE
	}
}

impl BenchmarkValue for PolkadotTransactionData {
	fn benchmark_value() -> Self {
		Self { encoded_extrinsic: ENCODED_EXTRINSIC.to_vec() }
	}
}

impl BenchmarkValue for PolkadotAccountId {
	fn benchmark_value() -> Self {
		Self::from_aliased(hex_literal::hex!(
			"858c1ee915090a119d4cb0774b908fa585ef7882f4648c577606490cc94f6e15"
		))
	}
}
impl BenchmarkValueExtended for PolkadotAccountId {
	fn benchmark_value_by_id(id: u8) -> Self {
		Self::from_aliased([id; 32])
	}
}

impl<E> BenchmarkValue for PolkadotApi<E> {
	fn benchmark_value() -> Self {
		PolkadotApi::CreateAnonymousVault(create_anonymous_vault::extrinsic_builder(
			PolkadotReplayProtection {
				genesis_hash: Default::default(),
				nonce: Default::default(),
			},
		))
	}
}

impl BenchmarkValue for EncodedPolkadotPayload {
	fn benchmark_value() -> Self {
		Self(hex_literal::hex!("02f87a827a6980843b9aca00843b9aca0082520894cfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcf808e646f5f736f6d657468696e672829c080a0b796e0276d89b0e02634d2f0cd5820e4af4bc0fcb76ecfcc4a3842e90d4b1651a07ab40be70e801fcd1e33460bfe34f03b8f390911658d49e58b0356a77b9432c0").to_vec())
	}
}

impl BenchmarkValue for TxId {
	fn benchmark_value() -> Self {
		Self { block_number: 32, extrinsic_index: 7 }
	}
}

impl BenchmarkValue for PolkadotTrackedData {
	fn benchmark_value() -> Self {
		PolkadotTrackedData { median_tip: 2 }
	}
}
