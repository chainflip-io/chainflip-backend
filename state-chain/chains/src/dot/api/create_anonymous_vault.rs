use codec::{Decode, Encode};
use scale_info::TypeInfo;

use crate::{
	dot::{
		Polkadot, PolkadotExtrinsicBuilder, PolkadotProxyType, PolkadotPublicKey,
		PolkadotReplayProtection, PolkadotRuntimeCall, ProxyCall,
	},
	impl_api_call_dot,
};

use crate::{ApiCall, ChainCrypto};
use sp_std::vec::Vec;

use sp_runtime::{traits::IdentifyAccount, MultiSigner, RuntimeDebug};

/// Represents all the arguments required to build the call to fetch assets for all given intent
/// ids.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct CreateAnonymousVault {
	/// The handler for creating and signing polkadot extrinsics
	pub extrinsic_builder: PolkadotExtrinsicBuilder,
	/// The proxy account public key that control the anonymous vault
	pub proxy_key: PolkadotPublicKey,
}

impl CreateAnonymousVault {
	pub fn new_unsigned(
		replay_protection: PolkadotReplayProtection,
		proxy_key: PolkadotPublicKey,
	) -> Self {
		let mut calldata = Self {
			extrinsic_builder: PolkadotExtrinsicBuilder::new_empty(
				replay_protection,
				MultiSigner::Sr25519(proxy_key.0).into_account(),
			),
			proxy_key,
		};
		// create and insert polkadot runtime call
		calldata
			.extrinsic_builder
			.insert_extrinsic_call(calldata.extrinsic_call_polkadot());
		// compute and insert the threshold signature payload
		calldata.extrinsic_builder.insert_threshold_signature_payload().expect(
			"This should not fail since SignedExtension of the SignedExtra type is implemented",
		);

		calldata
	}

	fn extrinsic_call_polkadot(&self) -> PolkadotRuntimeCall {
		PolkadotRuntimeCall::Proxy(ProxyCall::create_pure {
			proxy_type: PolkadotProxyType::Any,
			delay: 0,
			index: 0,
		})
	}
}

impl_api_call_dot!(CreateAnonymousVault);

#[cfg(test)]
mod test_create_anonymous_vault {

	use super::*;
	use crate::dot::{sr25519::Pair, NONCE_2, RAW_SEED_2, TEST_RUNTIME_VERSION};
	use sp_core::{crypto::Pair as TraitPair, Hasher};
	use sp_runtime::traits::BlakeTwo256;

	#[ignore]
	#[test]
	fn create_test_api_call() {
		let keypair_proxy: Pair = <Pair as TraitPair>::from_seed(&RAW_SEED_2);

		let create_anonymous_vault = CreateAnonymousVault::new_unsigned(
			PolkadotReplayProtection::new(NONCE_2, 0, TEST_RUNTIME_VERSION, Default::default()),
			PolkadotPublicKey(keypair_proxy.public()),
		);

		println!(
			"CallHash: 0x{}",
			create_anonymous_vault
				.extrinsic_builder
				.extrinsic_call
				.clone()
				.unwrap()
				.using_encoded(|encoded| hex::encode(BlakeTwo256::hash(encoded)))
		);
		println!(
			"Encoded Call: 0x{}",
			hex::encode(
				create_anonymous_vault
					.extrinsic_builder
					.extrinsic_call
					.clone()
					.unwrap()
					.encode()
			)
		);

		let create_anonymous_vault = create_anonymous_vault
			.clone()
			.signed(&keypair_proxy.sign(&create_anonymous_vault.threshold_signature_payload().0));
		assert!(create_anonymous_vault.is_signed());

		println!("encoded extrinsic: 0x{}", hex::encode(create_anonymous_vault.chain_encoded()));
	}
}
