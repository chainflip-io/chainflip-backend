use crate::{
	benchmarking_value::BenchmarkValue,
	dot::{
		BalancesCall, Polkadot, PolkadotAccountIdLookup, PolkadotAddress,
		PolkadotChargeTransactionPayment, PolkadotCheckMortality, PolkadotCheckNonce,
		PolkadotPublicKey, PolkadotRuntimeCall, PolkadotSignature, PolkadotSignedExtra,
		PolkadotTransactionData, PolkadotUncheckedExtrinsic,
	},
	eth::TrackedData,
};

use sp_core::{crypto::AccountId32, sr25519};
use sp_runtime::{generic::Era, MultiSignature};

const SIGNATURE: [u8; 64] = [1u8; 64];
const ACCOUNT_ID_1: [u8; 32] = [2u8; 32];
const ACCOUNT_ID_2: [u8; 32] = [3u8; 32];
const PUBLIC_KEY: [u8; 32] = [4u8; 32];
const NONCE: u32 = 5;
const ENCODED_EXTRINSIC: [u8; 100] = [3u8; 100];

impl BenchmarkValue for Option<PolkadotUncheckedExtrinsic> {
	fn benchmark_value() -> Self {
		Some(<PolkadotUncheckedExtrinsic as BenchmarkValue>::benchmark_value())
	}
}

impl BenchmarkValue for PolkadotUncheckedExtrinsic {
	fn benchmark_value() -> Self {
		PolkadotUncheckedExtrinsic::new_signed(
			PolkadotRuntimeCall::Balances(BalancesCall::transfer_all {
				dest: PolkadotAccountIdLookup::from(AccountId32::new(ACCOUNT_ID_1)),
				keep_alive: true,
			}),
			PolkadotAddress::Id(AccountId32::new(ACCOUNT_ID_2)),
			MultiSignature::Sr25519(sr25519::Signature(SIGNATURE)),
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
		sr25519::Signature(SIGNATURE)
	}
}

impl BenchmarkValue for PolkadotPublicKey {
	fn benchmark_value() -> Self {
		PolkadotPublicKey(sr25519::Public(PUBLIC_KEY))
	}
}

impl BenchmarkValue for TrackedData<Polkadot> {
	fn benchmark_value() -> Self {
		Self { block_height: 1000, base_fee: 10_000_000_000, priority_fee: 2_000_000_000 }
	}
}

impl BenchmarkValue for PolkadotTransactionData {
	fn benchmark_value() -> Self {
		Self { encoded_extrinsic: ENCODED_EXTRINSIC.to_vec() }
	}
}
