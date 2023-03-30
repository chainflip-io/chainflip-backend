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
	use crate::dot::{
		sr25519::Pair, NONCE_2, RAW_SEED_1, RAW_SEED_2, RAW_SEED_3, TEST_RUNTIME_VERSION,
	};
	use sp_core::crypto::{AccountId32, Pair as TraitPair};
	use sp_runtime::{app_crypto::Ss58Codec, traits::IdentifyAccount, MultiSigner};

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

		let payload = builder.get_signature_payload(
			TEST_RUNTIME_VERSION.spec_version,
			TEST_RUNTIME_VERSION.transaction_version,
		);
		assert_eq!(
			hex::encode(&payload.0),
			"
			1a02081d00002c8e8fde289aa5739f1b5a390404a4bdbc6a0588dce3f329d
			16f8a0ef6fa6bb701001a02081d01000c494f3eaa2263d95759e336c1090c
			e8710d25426e741cf9a3a218c93b14184700000000001d0200beb9c3f0ae5
			bda798dd3b65fe345fdf9031946849d8925ae7be73ee9407c673700000000
			000504000c494f3eaa2263d95759e336c10 90ce8710d25426e741cf9a3a2
			18c93b141847000048007c240000100000000000000000000000000000000
			0000000000000000000000000000000000000000000000000000000000000
			000000000000000000000000000000000000000000
			"
			.split_whitespace()
			.collect::<String>()
		);
		builder.insert_signature(
			keypair_old_proxy.public().into(),
			keypair_old_proxy.sign(&payload.0[..]),
		);
		assert!(builder.is_signed());
	}
}
