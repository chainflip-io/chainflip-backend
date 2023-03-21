use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_std::{boxed::Box, vec::Vec};

use crate::{
	dot::{
		BalancesCall, Polkadot, PolkadotAccountId, PolkadotAccountIdLookup,
		PolkadotExtrinsicBuilder, PolkadotProxyType, PolkadotReplayProtection, PolkadotRuntimeCall,
		ProxyCall, UtilityCall,
	},
	impl_api_call_dot,
};

use crate::{ApiCall, ChainCrypto, FetchAssetParams, TransferAssetParams};

use sp_runtime::RuntimeDebug;

/// Represents all the arguments required to build the call to fetch assets for all given intent
/// ids.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct BatchFetchAndTransfer {
	/// The handler for creating and signing polkadot extrinsics
	pub extrinsic_builder: PolkadotExtrinsicBuilder,
	/// The list of all inbound deposits that are to be fetched in this batch call.
	pub fetch_params: Vec<FetchAssetParams<Polkadot>>,
	/// The list of all outbound transfers that are to be executed in this call.
	pub transfer_params: Vec<TransferAssetParams<Polkadot>>,
	/// The vault anonymous Polkadot AccountId
	pub vault_account: PolkadotAccountId,
}

impl BatchFetchAndTransfer {
	pub fn new_unsigned(
		replay_protection: PolkadotReplayProtection,
		fetch_params: Vec<FetchAssetParams<Polkadot>>,
		transfer_params: Vec<TransferAssetParams<Polkadot>>,
		proxy_account: PolkadotAccountId,
		vault_account: PolkadotAccountId,
	) -> Self {
		let mut calldata = Self {
			extrinsic_builder: PolkadotExtrinsicBuilder::new_empty(
				replay_protection,
				proxy_account,
			),
			fetch_params,
			transfer_params,
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
		PolkadotRuntimeCall::Proxy(ProxyCall::proxy {
			real: PolkadotAccountIdLookup::from(self.vault_account.clone()),
			force_proxy_type: Some(PolkadotProxyType::Any),
			call: Box::new(PolkadotRuntimeCall::Utility(UtilityCall::batch {
				calls: [
					self.fetch_params
						.iter()
						.map(|fetch_param| {
							PolkadotRuntimeCall::Utility(UtilityCall::as_derivative {
								index: fetch_param.ingress_fetch_id as u16, /* todo: refer to
								                                             * issue #2354 */
								call: Box::new(PolkadotRuntimeCall::Balances(
									BalancesCall::transfer_all {
										dest: PolkadotAccountIdLookup::from(
											self.vault_account.clone(),
										),
										keep_alive: false,
									},
								)),
							})
						})
						.collect::<Vec<PolkadotRuntimeCall>>(),
					self.transfer_params
						.iter()
						.map(|transfer_param| {
							PolkadotRuntimeCall::Balances(BalancesCall::transfer {
								dest: PolkadotAccountIdLookup::from(transfer_param.to.clone()),
								value: transfer_param.amount,
							})
						})
						.collect::<Vec<PolkadotRuntimeCall>>(),
				]
				.concat(),
			})),
		})
	}
}

impl_api_call_dot!(BatchFetchAndTransfer);

#[cfg(test)]
mod test_batch_fetch {

	use super::*;
	use crate::dot::{sr25519::Pair, NONCE_1, RAW_SEED_1, RAW_SEED_2, TEST_RUNTIME_VERSION};
	use cf_primitives::chains::assets;
	use sp_core::{
		crypto::{AccountId32, Pair as TraitPair},
		sr25519, Hasher,
	};
	use sp_runtime::{
		traits::{BlakeTwo256, IdentifyAccount},
		MultiSigner,
	};

	#[ignore]
	#[test]
	fn create_test_api_call() {
		let keypair_vault: Pair = <Pair as TraitPair>::from_seed(&RAW_SEED_1);
		let account_id_vault: AccountId32 =
			MultiSigner::Sr25519(keypair_vault.public()).into_account();

		let keypair_proxy: Pair = <Pair as TraitPair>::from_seed(&RAW_SEED_2);
		let account_id_proxy: AccountId32 =
			MultiSigner::Sr25519(keypair_proxy.public()).into_account();

		let dummy_fetch_params: Vec<FetchAssetParams<Polkadot>> = vec![
			FetchAssetParams::<Polkadot> { ingress_fetch_id: 1, asset: assets::dot::Asset::Dot },
			FetchAssetParams::<Polkadot> { ingress_fetch_id: 2, asset: assets::dot::Asset::Dot },
			FetchAssetParams::<Polkadot> { ingress_fetch_id: 3, asset: assets::dot::Asset::Dot },
		];

		let dummy_transfer_params: Vec<TransferAssetParams<Polkadot>> = vec![
			TransferAssetParams::<Polkadot> {
				to: MultiSigner::Sr25519(sr25519::Public([7u8; 32])).into_account(),
				amount: 4,
				asset: assets::dot::Asset::Dot,
			},
			TransferAssetParams::<Polkadot> {
				to: MultiSigner::Sr25519(sr25519::Public([8u8; 32])).into_account(),
				amount: 5,
				asset: assets::dot::Asset::Dot,
			},
			TransferAssetParams::<Polkadot> {
				to: MultiSigner::Sr25519(sr25519::Public([9u8; 32])).into_account(),
				amount: 6,
				asset: assets::dot::Asset::Dot,
			},
		];

		let batch_fetch_api = BatchFetchAndTransfer::new_unsigned(
			PolkadotReplayProtection::new(NONCE_1, 0, TEST_RUNTIME_VERSION, Default::default()),
			dummy_fetch_params,
			dummy_transfer_params,
			account_id_proxy,
			account_id_vault,
		);

		println!(
			"CallHash: 0x{}",
			batch_fetch_api
				.extrinsic_builder
				.extrinsic_call
				.using_encoded(|encoded| hex::encode(BlakeTwo256::hash(encoded)))
		);
		println!(
			"Encoded Call: 0x{}",
			hex::encode(batch_fetch_api.extrinsic_builder.extrinsic_call.encode())
		);

		let batch_fetch_api = batch_fetch_api
			.clone()
			.signed(&keypair_proxy.sign(&batch_fetch_api.threshold_signature_payload().0));
		assert!(batch_fetch_api.is_signed());

		println!("encoded extrinsic: 0x{}", hex::encode(batch_fetch_api.chain_encoded()));
	}
}
