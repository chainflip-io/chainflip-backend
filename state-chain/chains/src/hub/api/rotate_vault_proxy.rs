use sp_std::{boxed::Box, vec};

use crate::dot::{
	PolkadotAccountId, PolkadotAccountIdLookup, PolkadotProxyType, PolkadotReplayProtection,
};

use crate::hub::{
	AssethubExtrinsicBuilder, AssethubRuntimeCall, BalancesCall, ProxyCall, UtilityCall,
};

pub fn extrinsic_builder(
	replay_protection: PolkadotReplayProtection,
	maybe_old_proxy: Option<PolkadotAccountId>,
	new_proxy: PolkadotAccountId,
	vault_account: PolkadotAccountId,
) -> AssethubExtrinsicBuilder {
	AssethubExtrinsicBuilder::new(
		replay_protection,
		AssethubRuntimeCall::Utility(UtilityCall::batch_all {
			calls: vec![
				AssethubRuntimeCall::Proxy(ProxyCall::proxy {
					real: PolkadotAccountIdLookup::from(vault_account),
					force_proxy_type: Some(PolkadotProxyType::Any),
					call: Box::new(AssethubRuntimeCall::Utility(UtilityCall::batch_all {
						calls: [
							Some(AssethubRuntimeCall::Proxy(ProxyCall::add_proxy {
								delegate: new_proxy.into(),
								proxy_type: PolkadotProxyType::Any,
								delay: 0,
							})),
							maybe_old_proxy.map(|old_proxy| {
								AssethubRuntimeCall::Proxy(ProxyCall::remove_proxy {
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
				AssethubRuntimeCall::Balances(BalancesCall::transfer_all {
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
	use crate::dot::{PolkadotPair, NONCE_2, RAW_SEED_2, RAW_SEED_3, TEST_RUNTIME_VERSION};

	#[test]
	fn create_test_api_call() {
		let keypair_old_proxy = PolkadotPair::from_seed(&RAW_SEED_2);
		let keypair_new_proxy = PolkadotPair::from_seed(&RAW_SEED_3);

		let mut builder = super::extrinsic_builder(
			PolkadotReplayProtection {
				nonce: NONCE_2,
				signer: keypair_old_proxy.public_key(),
				genesis_hash: Default::default(),
			},
			Some(keypair_old_proxy.public_key()),
			keypair_new_proxy.public_key(),
			PolkadotAccountId(hex_literal::hex!(
				"2c8e8fde289aa5739f1b5a390404a4bdbc6a0588dce3f329d16f8a0ef6fa6bb7"
			)),
		);

		let payload = builder.get_signature_payload(
			TEST_RUNTIME_VERSION.spec_version,
			TEST_RUNTIME_VERSION.transaction_version,
		);
		assert_eq!(
			payload.0,
			hex_literal::hex!(
				"
				1a02081d00002c8e8fde289aa5739f1b5a390404a4bdbc6a0588dce3f329d16f8a0ef6fa6bb70100
				1a02081d01000c494f3eaa2263d95759e336c1090ce8710d25426e741cf9a3a218c93b1418470000
				0000001d0200beb9c3f0ae5bda798dd3b65fe345fdf9031946849d8925ae7be73ee9407c67370000
				0000000504000c494f3eaa2263d95759e336c1090ce8710d25426e741cf9a3a218c93b1418470000
				4800007c240000100000000000000000000000000000000000000000000000000000000000000000
				000000000000000000000000000000000000000000000000000000000000000000000000
			"
			)
		);
		builder.insert_signer_and_signature(
			keypair_old_proxy.public_key(),
			keypair_old_proxy.sign(&payload),
		);
		assert!(builder.is_signed());
	}
}