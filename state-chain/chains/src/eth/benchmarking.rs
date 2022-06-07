#![cfg_attr(not(feature = "std"), no_std)]
use crate::eth::{
	to_ethereum_address, Address, AggKey, RawSignedTransaction, SchnorrVerificationComponents, H256,
};

use crate::eth::{
	api::{set_agg_key_with_agg_key::SetAggKeyWithAggKey, EthereumApi},
	EthereumReplayProtection, SigData,
};

use sp_std::vec;

use crate::eth::sig_constants::{SIG, SIG_NONCE};

use crate::{
	benchmarking_value::BenchmarkValue,
	eth::sig_constants::{AGG_KEY_PUB, MSG_HASH},
};

use libsecp256k1::{PublicKey, SecretKey};

/// Returns a valid signature for use in benchmarks.
impl BenchmarkValue for SchnorrVerificationComponents {
	fn benchmark_value() -> Self {
		let k = SecretKey::parse(&SIG_NONCE).expect("Valid signature nonce");
		let k_times_g_address = to_ethereum_address(PublicKey::from_secret_key(&k));
		Self { s: SIG, k_times_g_address }
	}
}

impl BenchmarkValue for Address {
	fn benchmark_value() -> Self {
		to_ethereum_address(PublicKey::from_secret_key(&SecretKey::parse(&SIG_NONCE).unwrap()))
			.into()
	}
}

impl BenchmarkValue for H256 {
	fn benchmark_value() -> Self {
		MSG_HASH.into()
	}
}

impl BenchmarkValue for RawSignedTransaction {
	fn benchmark_value() -> Self {
		MSG_HASH.to_vec()
	}
}

impl BenchmarkValue for AggKey {
	fn benchmark_value() -> Self {
		AggKey::from_pubkey_compressed(AGG_KEY_PUB)
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValue for EthereumApi {
	#[cfg(feature = "runtime-benchmarks")]
	fn benchmark_value() -> Self {
		let key = AggKey::from_pubkey_compressed(AGG_KEY_PUB);
		let mut sig_data = SigData::new_empty(EthereumReplayProtection {
			key_manager_address: hex_literal::hex!("5FbDB2315678afecb367f032d93F642f64180aa3"),
			chain_id: 31337,
			nonce: 15,
		});
		sig_data.insert_signature(&SchnorrVerificationComponents::benchmark_value());
		sig_data.insert_msg_hash_from(&MSG_HASH);
		EthereumApi::SetAggKeyWithAggKey(SetAggKeyWithAggKey { sig_data, new_key: key })
	}
}
