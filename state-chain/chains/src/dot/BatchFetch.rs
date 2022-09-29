use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_std::{boxed::Box, vec, vec::Vec};

pub use pallet_balances::Call as BalancesCall;
pub use pallet_proxy::{AccountIdLookupOf, Call as ProxyCall};
pub use pallet_utility::Call as UtilityCall;

use polkadot_runtime::ProxyType;

use crate::{ApiCall, ChainAbi, ChainCrypto, Polkadot, PolkadotRuntimeCall};

//use crate::TransferAssetParams;

use super::{ethabi_function, ethabi_param, EthereumReplayProtection};

use sp_runtime::RuntimeDebug;

//pub type TransferDotParams = crate::TransferAssetParams<Polkadot>;

pub type IntentId = u16;

pub type PolkadotAccountIdLookupOf = AccountIdLookup<PolkadotRuntime::AccountId, ()>::Source; //import this struct from traits.rs in polkadot runtime primitives repo

/// Represents all the arguments required to build the call to Vault's 'allBatch'
/// function.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, Default, PartialEq, Eq)]
pub struct BatchFetch {
	/// The signature data for validation and replay protection.
	pub extrinsic_signature_handler: PolkadotExtrinsicSignatureHandler,
	/// The list of all inbound deposits that are to be fetched in this batch call.
	pub intent_ids: Vec<IntentId>,
}

impl BatchFetch {
	pub fn new_unsigned(
		nonce: <PolkadotRuntime as frame_system::Config>::Index,
		intent_ids: Vec<IntentId>,
		vault_account: Polkadot::ChainAccount,
	) -> Self {
		let mut calldata = Self {
			extrinsic_signature_handler: PolkadotExtrinsicSignatureHandler::new_empty(
				nonce,
				vault_account,
			),
			intent_ids,
		};
		calldata
			.extrinsic_signature_handler
			.insert_extrinsic_call(calldata.extrinsic_call_polkadot());

		calldata
	}

	fn extrinsic_call_polkadot(&self) -> PolkadotRuntimeCall {
		let batch_fetch_call = PolkadotRuntimeCall::Proxy(ProxyCall::proxy {
			real: PolkadotAccountIdLookupOf::from(self.extrinsic_signature_handler.vault_account),
			force_proxy_type: Some(ProxyType::default()), //default ProxyType is ProxyType::Any
			call: Box::new(PolkadotRuntimeCall::Utility(UtilityCall::batch {
				calls: intent_ids
					.iter()
					.map(|intent_id| {
						PolkadotRuntimeCall::Utility(UtilityCall::as_derivative {
							index: intent_id,
							call: Box::new(PolkadotRuntimeCall::Balances(
								BalancesCall::transfer_all {
									dest: PolkadotAccountIdLookupOf::from(
										self.extrinsic_signature_handler.vault_account,
									),
									keep_alive: false,
								},
							)),
						});
					})
					.collect::<Vec<PolkadotRuntimeCall>>(),
			})),
		});
	}
}

impl ApiCall<Polkadot> for BatchFetch {
	fn threshold_signature_payload(&self) -> <Polkadot as ChainCrypto>::Payload {
		self.extrinsic_signature_handler.insert_and_get_threshold_signature_payload()
	}

	fn signed(mut self, signature: &<Polkadot as ChainCrypto>::ThresholdSignature) -> Self {
		self.extrinsic_signature_handler
			.insert_and_get_signed_unchecked_extrinsic(signature);
		self
	}

	fn chain_encoded(&self) -> <Polkadot as ChainAbi>::SignedTransaction {
		self.extrinsic_signature_handler.signed_extrinsic
	}

	fn is_signed(&self) -> bool {
		self.extrinsic_signature_handler.is_signed()
	}
}
