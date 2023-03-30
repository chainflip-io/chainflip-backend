use sp_std::{boxed::Box, vec};

use crate::dot::{
	BalancesCall, PolkadotAccountId, PolkadotAccountIdLookup, PolkadotExtrinsicBuilder,
	PolkadotProxyType, PolkadotReplayProtection, PolkadotRuntimeCall, ProxyCall, UtilityCall,
};

pub fn extrinsic_builder(
	replay_protection: PolkadotReplayProtection,
	maybe_old_proxy: Option<PolkadotAccountId>,
	new_proxy: PolkadotAccountId,
	vault_account: PolkadotAccountId,
) -> PolkadotExtrinsicBuilder {
	PolkadotExtrinsicBuilder::new(
		replay_protection,
		PolkadotRuntimeCall::Utility(UtilityCall::batch_all {
			calls: vec![
				PolkadotRuntimeCall::Proxy(ProxyCall::proxy {
					real: PolkadotAccountIdLookup::from(vault_account),
					force_proxy_type: Some(PolkadotProxyType::Any),
					call: Box::new(PolkadotRuntimeCall::Utility(UtilityCall::batch_all {
						calls: [
							Some(PolkadotRuntimeCall::Proxy(ProxyCall::add_proxy {
								delegate: new_proxy.clone().into(),
								proxy_type: PolkadotProxyType::Any,
								delay: 0,
							})),
							maybe_old_proxy.map(|old_proxy| {
								PolkadotRuntimeCall::Proxy(ProxyCall::remove_proxy {
									delegate: old_proxy.into(),
									proxy_type: PolkadotProxyType::Any,
									delay: 0,
								})
							}),
						]
						.into_iter()
						.flatten()
						.collect(),
					})),
				}),
				PolkadotRuntimeCall::Balances(BalancesCall::transfer_all {
					dest: new_proxy.into(),
					keep_alive: false,
				}),
			],
		}),
	)
}

#[cfg(test)]
mod test_rotate_vault_proxy {

	use super::*;
	use crate::{
		dot::{
			api::{mocks::MockEnv, WithEnvironment},
			sr25519::Pair,
			NONCE_2, RAW_SEED_1, RAW_SEED_2, RAW_SEED_3, TEST_RUNTIME_VERSION,
		},
		ApiCall,
	};
	use codec::Encode;
	use sp_core::{
		crypto::{AccountId32, Pair as TraitPair},
		Hasher,
	};
	use sp_runtime::{
		app_crypto::Ss58Codec,
		traits::{BlakeTwo256, IdentifyAccount},
		MultiSigner,
	};

	#[ignore]
	#[test]
	fn create_test_api_call() {
		let keypair_vault: Pair = <Pair as TraitPair>::from_seed(&RAW_SEED_1);
		let _account_id_vault: AccountId32 =
			MultiSigner::Sr25519(keypair_vault.public()).into_account();

		let keypair_old_proxy: Pair = <Pair as TraitPair>::from_seed(&RAW_SEED_2);
		let _account_id_old_proxy: AccountId32 =
			MultiSigner::Sr25519(keypair_old_proxy.public()).into_account();

		let keypair_new_proxy: Pair = <Pair as TraitPair>::from_seed(&RAW_SEED_3);
		let _account_id_new_proxy: AccountId32 =
			MultiSigner::Sr25519(keypair_new_proxy.public()).into_account();

		let mut builder = super::extrinsic_builder(
			PolkadotReplayProtection { nonce: NONCE_2, genesis_hash: Default::default() },
			Some(keypair_old_proxy.public().into()),
			keypair_new_proxy.public().into(),
			AccountId32::from_ss58check("5D58KA25o2KcL9EiBJckjScGzvH5nUEiKJBrgAjsSfRuGJkc")
				.unwrap(),
		);

		let encoded_call = builder.extrinsic_call.encode();
		println!("CallHash: 0x{}", hex::encode(BlakeTwo256::hash(&encoded_call[..])));
		println!("Encoded Call: 0x{}", hex::encode(encoded_call));

		let payload = builder.get_signature_payload(
			TEST_RUNTIME_VERSION.spec_version,
			TEST_RUNTIME_VERSION.transaction_version,
		);
		builder.insert_signature(
			keypair_old_proxy.public().into(),
			keypair_old_proxy.sign(&payload.0[..]),
		);
		assert!(builder.is_signed());

		println!(
			"encoded extrinsic: 0x{}",
			hex::encode(builder.with_environment::<MockEnv>().chain_encoded())
		);
	}
}
