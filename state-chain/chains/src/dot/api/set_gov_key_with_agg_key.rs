use cf_primitives::{chains::Polkadot, PolkadotAccountId};
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_runtime::{traits::IdentifyAccount, MultiSigner, RuntimeDebug};

use sp_std::{boxed::Box, vec, vec::Vec};

use crate::{
	dot::{
		PolkadotAccountIdLookup, PolkadotExtrinsicBuilder, PolkadotProxyType, PolkadotPublicKey,
		PolkadotReplayProtection, PolkadotRuntimeCall, ProxyCall, UtilityCall,
	},
	impl_api_call_dot, ApiCall, ChainCrypto,
};

/// The controller of the Polkadot vault account is executing this extrinsic and able
/// to change the proxy.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct ChangeGovKey {
	pub extrinsic_builder: PolkadotExtrinsicBuilder,
	/// The current proxy AccountId
	pub old_key: Option<PolkadotPublicKey>,
	/// The current proxy AccountId
	pub new_key: PolkadotPublicKey,
	/// The vault anonymous Polkadot AccountId
	pub vault_account: PolkadotAccountId,
}

impl ChangeGovKey {
	pub fn new_unsigned(
		replay_protection: PolkadotReplayProtection,
		old_key: Option<PolkadotPublicKey>,
		new_key: PolkadotPublicKey,
		vault_account: PolkadotAccountId,
	) -> Self {
		let the_vault_key: PolkadotPublicKey = PolkadotPublicKey::default();
		let mut calldata = Self {
			extrinsic_builder: PolkadotExtrinsicBuilder::new_empty(
				replay_protection,
				MultiSigner::Sr25519(the_vault_key.0).into_account(),
			),
			old_key,
			new_key,
			vault_account,
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
		// TODO: not sure if any is the right choice for this - need to figure that out
		PolkadotRuntimeCall::Proxy(ProxyCall::proxy {
			real: PolkadotAccountIdLookup::from(self.vault_account.clone()),
			force_proxy_type: Some(PolkadotProxyType::Any),
			call: Box::new(PolkadotRuntimeCall::Utility(UtilityCall::batch_all {
				calls: if self.old_key.is_some() {
					vec![
						PolkadotRuntimeCall::Proxy(ProxyCall::add_proxy {
							delegate: PolkadotAccountIdLookup::from(
								MultiSigner::Sr25519(self.new_key.0).into_account(),
							),
							proxy_type: PolkadotProxyType::Any,
							delay: 0,
						}),
						PolkadotRuntimeCall::Proxy(ProxyCall::remove_proxy {
							delegate: PolkadotAccountIdLookup::from(
								MultiSigner::Sr25519(
									self.old_key.expect("old key to be available").0,
								)
								.into_account(),
							),
							proxy_type: PolkadotProxyType::Any,
							delay: 0,
						}),
					]
				} else {
					vec![PolkadotRuntimeCall::Proxy(ProxyCall::add_proxy {
						delegate: PolkadotAccountIdLookup::from(
							MultiSigner::Sr25519(self.new_key.0).into_account(),
						),
						proxy_type: PolkadotProxyType::Any,
						delay: 0,
					})]
				},
			})),
		})
	}
}

impl_api_call_dot!(ChangeGovKey);
