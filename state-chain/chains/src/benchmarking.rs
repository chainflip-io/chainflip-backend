use crate::eth::{to_ethereum_address, SchnorrVerificationComponents, TransactionHash, H256};
use cf_runtime_benchmark_utilities::BenchmarkDefault;
use libsecp256k1::{PublicKey, SecretKey};

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

// impl<T: std::default::Default> BenchmarkDefault for TransactionHash {
// 	fn benchmark_default() -> Self {
// 		H256::from([0u8; 32])
// 	}
// }
