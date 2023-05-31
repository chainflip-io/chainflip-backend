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
	use crate::dot::{PolkadotPair, NONCE_2, RAW_SEED_2, TEST_RUNTIME_VERSION};

	#[test]
	fn create_test_api_call() {
		let keypair_proxy = PolkadotPair::from_seed(&RAW_SEED_2);

		let mut builder = super::extrinsic_builder(PolkadotReplayProtection {
			nonce: NONCE_2,
			genesis_hash: Default::default(),
		});

		let payload = builder.get_signature_payload(
			TEST_RUNTIME_VERSION.spec_version,
			TEST_RUNTIME_VERSION.transaction_version,
		);
		assert_eq!(
			hex::encode(&payload.0),
			"
			1d04000000000000000048007c24000010000000000000000000000000000000
			0000000000000000000000000000000000000000000000000000000000000000
			0000000000000000000000000000000000000000
			"
			.split_whitespace()
			.collect::<String>()
		);
		builder.insert_signature(keypair_proxy.public_key(), keypair_proxy.sign(&payload));
		assert!(builder.is_signed());
	}
}
