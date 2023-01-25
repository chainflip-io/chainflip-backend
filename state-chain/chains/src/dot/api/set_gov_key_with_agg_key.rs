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
	ApiCall,
};

/// The controller of the Polkadot vault account is executing this extrinsic and able
/// to change the proxy.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct ChangeGovKey {
	pub extrinsic_handler: PolkadotExtrinsicBuilder,
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
			extrinsic_handler: PolkadotExtrinsicBuilder::new_empty(
				replay_protection,
				MultiSigner::Sr25519(the_vault_key.0).into_account(),
			),
			old_key,
			new_key,
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

impl ApiCall<Polkadot> for ChangeGovKey {
	fn threshold_signature_payload(&self) -> <Polkadot as crate::ChainCrypto>::Payload {
		self
		.extrinsic_handler
		.signature_payload
		.clone()
		.expect("This should never fail since the apicall created above with new_unsigned() ensures it exists")
	}

	fn signed(
		mut self,
		threshold_signature: &<Polkadot as crate::ChainCrypto>::ThresholdSignature,
	) -> Self {
		self.extrinsic_handler
			.insert_signature_and_get_signed_unchecked_extrinsic(threshold_signature.clone());
		self
	}

	fn chain_encoded(&self) -> Vec<u8> {
		self.extrinsic_handler.signed_extrinsic.clone().unwrap().encode()
	}

	fn is_signed(&self) -> bool {
		self.extrinsic_handler.is_signed().unwrap_or(false)
	}
}
