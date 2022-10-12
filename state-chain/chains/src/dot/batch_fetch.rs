use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_std::{boxed::Box, vec::Vec};

use crate::dot::{
	BalancesCall, Polkadot, PolkadotAccountIdLookup, PolkadotExtrinsicHandler, PolkadotIndex,
	PolkadotProxyType, PolkadotRuntimeCall, ProxyCall, UtilityCall,
};

use crate::{ApiCall, Chain, ChainAbi, ChainCrypto};

use sp_runtime::RuntimeDebug;

pub type IntentId = u16;

/// Represents all the arguments required to build the call to fetch assets for all given intent
/// ids.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct BatchFetch {
	/// The hancler for creating and signing polkadot extrinsics
	pub extrinsic_handler: PolkadotExtrinsicHandler,
	/// The list of all inbound deposits that are to be fetched in this batch call.
	pub intent_ids: Vec<IntentId>,
}

impl BatchFetch {
	pub fn new_unsigned(
		nonce: PolkadotIndex,
		intent_ids: Vec<IntentId>,
		vault_account: <Polkadot as Chain>::ChainAccount,
	) -> Self {
		let mut calldata = Self {
			extrinsic_handler: PolkadotExtrinsicHandler::new_empty(nonce, vault_account),
			intent_ids,
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
		PolkadotRuntimeCall::Proxy(ProxyCall::proxy {
			real: PolkadotAccountIdLookup::from(self.extrinsic_handler.vault_account.clone()),
			force_proxy_type: Some(PolkadotProxyType::Any),
			call: Box::new(PolkadotRuntimeCall::Utility(UtilityCall::batch {
				calls: self
					.intent_ids
					.iter()
					.map(|intent_id| {
						PolkadotRuntimeCall::Utility(UtilityCall::as_derivative {
							index: *intent_id,
							call: Box::new(PolkadotRuntimeCall::Balances(
								BalancesCall::transfer_all {
									dest: PolkadotAccountIdLookup::from(
										self.extrinsic_handler.vault_account.clone(),
									),
									keep_alive: false,
								},
							)),
						})
					})
					.collect::<Vec<PolkadotRuntimeCall>>(),
			})),
		})
	}
}

impl ApiCall<Polkadot> for BatchFetch {
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
		self.extrinsic_handler.signed_extrinsic.clone()
	}

	fn is_signed(&self) -> bool {
		self.extrinsic_handler.is_signed().unwrap_or(false)
	}
}
