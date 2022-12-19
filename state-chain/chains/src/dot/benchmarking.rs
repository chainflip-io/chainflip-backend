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
use sp_runtime::{generic::Era, traits::IdentifyAccount, MultiSignature, MultiSigner};

use super::{
	api::{create_anonymous_vault::CreateAnonymousVault, PolkadotApi},
	EncodedPolkadotPayload, PolkadotAccountId, PolkadotReplayProtection, TxId,
};

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

impl BenchmarkValue for PolkadotAccountId {
	fn benchmark_value() -> Self {
		MultiSigner::Sr25519(sr25519::Public(hex_literal::hex!(
			"858c1ee915090a119d4cb0774b908fa585ef7882f4648c577606490cc94f6e15"
		)))
		.into_account()
	}
}

impl<E> BenchmarkValue for PolkadotApi<E> {
	fn benchmark_value() -> Self {
		PolkadotApi::CreateAnonymousVault(CreateAnonymousVault::new_unsigned(
			PolkadotReplayProtection {
				polkadot_config: Default::default(),
				nonce: Default::default(),
				tip: Default::default(),
			},
			PolkadotPublicKey::benchmark_value(),
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
