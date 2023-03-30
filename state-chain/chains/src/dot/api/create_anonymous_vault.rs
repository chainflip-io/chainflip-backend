use crate::dot::{
	PolkadotExtrinsicBuilder, PolkadotProxyType, PolkadotReplayProtection, PolkadotRuntimeCall,
	ProxyCall,
};

pub fn extrinsic_builder(replay_protection: PolkadotReplayProtection) -> PolkadotExtrinsicBuilder {
	PolkadotExtrinsicBuilder::new(
		replay_protection,
		PolkadotRuntimeCall::Proxy(ProxyCall::create_pure {
			proxy_type: PolkadotProxyType::Any,
			delay: 0,
			index: 0,
		}),
	)
}

#[cfg(test)]
mod test_create_anonymous_vault {

	use super::*;
	use crate::{
		dot::{
			api::{mocks::MockEnv, WithEnvironment},
			sr25519::Pair,
			NONCE_2, RAW_SEED_2, TEST_RUNTIME_VERSION,
		},
		ApiCall,
	};
	use codec::Encode;
	use sp_core::{crypto::Pair as TraitPair, Hasher};
	use sp_runtime::traits::BlakeTwo256;

	#[ignore]
	#[test]
	fn create_test_api_call() {
		let keypair_proxy: Pair = <Pair as TraitPair>::from_seed(&RAW_SEED_2);

		let mut builder = super::extrinsic_builder(PolkadotReplayProtection {
			nonce: NONCE_2,
			genesis_hash: Default::default(),
		});

		let encoded_call = builder.extrinsic_call.encode();
		println!("CallHash: 0x{}", hex::encode(BlakeTwo256::hash(&encoded_call[..])));
		println!("Encoded Call: 0x{}", hex::encode(encoded_call));

		let payload = builder.get_signature_payload(
			TEST_RUNTIME_VERSION.spec_version,
			TEST_RUNTIME_VERSION.transaction_version,
		);
		builder.insert_signature(keypair_proxy.public().into(), keypair_proxy.sign(&payload.0[..]));
		assert!(builder.is_signed());

		println!(
			"encoded extrinsic: 0x{}",
			hex::encode(builder.with_environment::<MockEnv>().chain_encoded())
		);
	}
}
