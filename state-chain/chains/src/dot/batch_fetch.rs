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
		self.extrinsic_handler.signed_extrinsic.clone().encode()
	}

	fn is_signed(&self) -> bool {
		self.extrinsic_handler.is_signed().unwrap_or(false)
	}
}

#[cfg(test)]
mod test_batch_fetch {

	use super::*;
	use crate::dot::sr25519::Pair;
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
	pub const NONCE_1: u32 = 4; //correct nonce has to be provided for this account (see/track onchain)

	#[ignore]
	#[test]
	fn create_test_api_call() {
		let keypair_1: Pair = <Pair as TraitPair>::from_seed(&RAW_SEED_1);
		let account_id_1: AccountId32 = MultiSigner::Sr25519(keypair_1.public()).into_account();

		let dummy_intent_ids: Vec<u16> = vec![1, 2, 3];

		let batch_fetch_api = BatchFetch::new_unsigned(NONCE_1, dummy_intent_ids, account_id_1);

		println!(
			"CallHash: 0x{}",
			batch_fetch_api
				.extrinsic_handler
				.extrinsic_call
				.using_encoded(|encoded| hex::encode(BlakeTwo256::hash(encoded)))
		);
		println!(
			"Encoded Call: 0x{}",
			hex::encode(batch_fetch_api.extrinsic_handler.extrinsic_call.encode())
		);

		let batch_fetch_api = batch_fetch_api
			.clone()
			.signed(&keypair_1.sign(&batch_fetch_api.threshold_signature_payload()));
		assert!(batch_fetch_api.is_signed());

		println!("encoded extrinsic: 0x{}", hex::encode(batch_fetch_api.chain_encoded()));
	}
}
