#![cfg_attr(not(feature = "std"), no_std)]
use crate::{
	benchmarking::EthereumApi::SetAggKeyWithAggKey,
	eth::{
		api::EthereumApi, to_ethereum_address, SchnorrVerificationComponents, TransactionHash, H256,
	},
};

use crate::benchmarking_default::BenchmarkDefault;

use ethabi::Address;
use libsecp256k1::{PublicKey, SecretKey};

// #[cfg(not(feature = "runtime-benchmarks"))]
// impl<T> BenchmarkDefault for T {}

/// Returns a valid signature for use in benchmarks.
impl BenchmarkDefault for SchnorrVerificationComponents {
	fn benchmark_default() -> Self {
		const SIG: [u8; 32] =
			hex_literal::hex!("beb37e87509e15cd88b19fa224441c56acc0e143cb25b9fd1e57fdafed215538");
		const SIG_NONCE: [u8; 32] =
			hex_literal::hex!("d51e13c68bf56155a83e50fd9bc840e2a1847fb9b49cd206a577ecd1cd15e285");
		let k = SecretKey::parse(&SIG_NONCE).expect("Valid signature nonce");
		let k_times_g_address = to_ethereum_address(PublicKey::from_secret_key(&k));

		Self { s: SIG, k_times_g_address }
	}
}

#[cfg(not(feature = "runtime-benchmarks"))]
impl BenchmarkDefault for Address {
	fn benchmark_default() -> Self {
		const SIG_NONCE: [u8; 32] =
			hex_literal::hex!("d51e13c68bf56155a83e50fd9bc840e2a1847fb9b49cd206a577ecd1cd15e285");
		to_ethereum_address(PublicKey::from_secret_key(&SecretKey::parse(&SIG_NONCE).unwrap()))
			.into()
	}
}

#[cfg(not(feature = "runtime-benchmarks"))]
impl BenchmarkDefault for H256 {
	fn benchmark_default() -> Self {
		const SIG_NONCE: [u8; 32] =
			hex_literal::hex!("d51e13c68bf56155a83e50fd9bc840e2a1847fb9b49cd206a577ecd1cd15e285");
		SIG_NONCE.into()
	}
}
