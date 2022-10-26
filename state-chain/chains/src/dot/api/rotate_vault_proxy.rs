use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_std::{boxed::Box, vec};

use crate::dot::{
	BalancesCall, Polkadot, PolkadotAccountId, PolkadotAccountIdLookup, PolkadotExtrinsicBuilder,
	PolkadotProxyType, PolkadotPublicKey, PolkadotReplayProtection, PolkadotRuntimeCall, ProxyCall,
	UtilityCall,
};

use crate::{ApiCall, ChainAbi, ChainCrypto};

use sp_runtime::{traits::IdentifyAccount, MultiSigner, RuntimeDebug};

/// Represents all the arguments required to build the call to fetch assets for all given intent
/// ids.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct RotateVaultProxy {
	/// The handler for creating and signing polkadot extrinsics
	pub extrinsic_handler: PolkadotExtrinsicBuilder,
	/// The new proxy account public key
	pub new_proxy: PolkadotPublicKey,
	/// The old proxy account public Key
	pub old_proxy: PolkadotPublicKey,
	/// The vault anonymous Polkadot AccountId
	pub vault_account: PolkadotAccountId,
}

impl RotateVaultProxy {
	pub fn new_unsigned(
		replay_protection: PolkadotReplayProtection,
		new_proxy: PolkadotPublicKey,
		old_proxy: PolkadotPublicKey,
		vault_account: PolkadotAccountId,
	) -> Self {
		let mut calldata = Self {
			extrinsic_handler: PolkadotExtrinsicBuilder::new_empty(
				replay_protection,
				MultiSigner::Sr25519(old_proxy.0).into_account(),
			),
			new_proxy,
			old_proxy,
			vault_account,
		};
		// create and insert polkadot runtime call
		calldata
			.extrinsic_handler
			.insert_extrinsic_call(calldata.extrinsic_call_polkadot());
		// compute and insert the threshold signature payload
		calldata.extrinsic_handler.insert_threshold_signature_payload().expect(
			"This should not fail since SignedExtension of the SignedExtra type is implemented",
		);

		calldata
	}

	fn extrinsic_call_polkadot(&self) -> PolkadotRuntimeCall {
		PolkadotRuntimeCall::Utility(UtilityCall::batch_all {
			calls: vec![
				PolkadotRuntimeCall::Proxy(ProxyCall::proxy {
					real: PolkadotAccountIdLookup::from(self.vault_account.clone()),
					force_proxy_type: Some(PolkadotProxyType::Any),
					call: Box::new(PolkadotRuntimeCall::Utility(UtilityCall::batch_all {
						calls: vec![
							PolkadotRuntimeCall::Proxy(ProxyCall::add_proxy {
								delegate: PolkadotAccountIdLookup::from(
									MultiSigner::Sr25519(self.new_proxy.0).into_account(),
								),
								proxy_type: PolkadotProxyType::Any,
								delay: 0,
							}),
							PolkadotRuntimeCall::Proxy(ProxyCall::remove_proxy {
								delegate: PolkadotAccountIdLookup::from(
									MultiSigner::Sr25519(self.old_proxy.0).into_account(),
								),
								proxy_type: PolkadotProxyType::Any,
								delay: 0,
							}),
						],
					})),
				}),
				PolkadotRuntimeCall::Balances(BalancesCall::transfer_all {
					dest: PolkadotAccountIdLookup::from(
						MultiSigner::Sr25519(self.new_proxy.0).into_account(),
					),
					keep_alive: false,
				}),
			],
		})
	}
}

impl ApiCall<Polkadot> for RotateVaultProxy {
	fn threshold_signature_payload(&self) -> <Polkadot as ChainCrypto>::Payload {
		self
		.extrinsic_handler
		.signature_payload
		.clone()
		.expect("This should never fail since the apicall created above with new_unsigned() ensures it exists")
	}

	fn signed(mut self, signature: &<Polkadot as ChainCrypto>::ThresholdSignature) -> Self {
		self.extrinsic_handler
			.insert_signature_and_get_signed_unchecked_extrinsic(signature.clone());
		self
	}

	fn chain_encoded(&self) -> <Polkadot as ChainAbi>::SignedTransaction {
		self.extrinsic_handler.signed_extrinsic.clone().unwrap().encode()
	}

	fn is_signed(&self) -> bool {
		self.extrinsic_handler.is_signed().unwrap_or(false)
	}
}

#[cfg(test)]
mod test_rotate_vault_proxy {

	use super::*;
	use crate::dot::{sr25519::Pair, NetworkChoice};
	use sp_core::{
		crypto::{AccountId32, Pair as TraitPair},
		Hasher,
	};
	use sp_runtime::{
		traits::{BlakeTwo256, IdentifyAccount},
		MultiSigner,
	};

	// test westend account 1 (CHAINFLIP-TEST)
	// address: "5E2WfQFeafdktJ5AAF6ZGZ71Yj4fiJnHWRomVmeoStMNhoZe"
	pub const RAW_SEED_1: [u8; 32] =
		hex_literal::hex!("858c1ee915090a119d4cb0774b908fa585ef7882f4648c577606490cc94f6e15");
	pub const _NONCE_1: u32 = 11; //correct nonce has to be provided for this account (see/track onchain)

	// test westend account 2 (CHAINFLIP-TEST-2)
	// address: "5GNn92C9ngX4sNp3UjqGzPbdRfbbV8hyyVVNZaH2z9e5kzxA"
	pub const RAW_SEED_2: [u8; 32] =
		hex_literal::hex!("4b734882accd7a0e27b8b0d3cb7db79ab4da559d1d5f84f35fd218a1ee12ece4");
	pub const NONCE_2: u32 = 0; //correct nonce has to be provided for this account (see/track onchain)

	// test westend account 3 (CHAINFLIP-TEST-3)
	// address: "5CLpD6DBg2hFToBJYKDB7bPVAf4TKw2F1Q2xbnzdHSikH3uK"
	pub const RAW_SEED_3: [u8; 32] =
		hex_literal::hex!("ce7fec0dd410141c04e246a91f7ac909aa9707b56a8ecd33e794a49f1b5d70e6");
	pub const _NONCE_3: u32 = 0; //correct nonce has to be provided for this account (see/track onchain)

	#[ignore]
	#[test]
	fn create_test_api_call() {
		let keypair_vault: Pair = <Pair as TraitPair>::from_seed(&RAW_SEED_1);
		let account_id_vault: AccountId32 =
			MultiSigner::Sr25519(keypair_vault.public()).into_account();

		let keypair_old_proxy: Pair = <Pair as TraitPair>::from_seed(&RAW_SEED_2);
		let _account_id_old_proxy: AccountId32 =
			MultiSigner::Sr25519(keypair_old_proxy.public()).into_account();

		let keypair_new_proxy: Pair = <Pair as TraitPair>::from_seed(&RAW_SEED_3);
		let _account_id_new_proxy: AccountId32 =
			MultiSigner::Sr25519(keypair_new_proxy.public()).into_account();

		let rotate_vault_proxy_api = RotateVaultProxy::new_unsigned(
			PolkadotReplayProtection::new(NONCE_2, 0, NetworkChoice::WestendTestnet),
			PolkadotPublicKey(keypair_new_proxy.public()),
			PolkadotPublicKey(keypair_old_proxy.public()),
			account_id_vault,
		);

		println!(
			"CallHash: 0x{}",
			rotate_vault_proxy_api
				.extrinsic_handler
				.extrinsic_call
				.clone()
				.unwrap()
				.using_encoded(|encoded| hex::encode(BlakeTwo256::hash(encoded)))
		);
		println!(
			"Encoded Call: 0x{}",
			hex::encode(
				rotate_vault_proxy_api
					.extrinsic_handler
					.extrinsic_call
					.clone()
					.unwrap()
					.encode()
			)
		);

		let rotate_vault_proxy_api = rotate_vault_proxy_api
			.clone()
			.signed(&keypair_old_proxy.sign(&rotate_vault_proxy_api.threshold_signature_payload()));
		assert!(rotate_vault_proxy_api.is_signed());

		println!("encoded extrinsic: 0x{}", hex::encode(rotate_vault_proxy_api.chain_encoded()));
	}
}
