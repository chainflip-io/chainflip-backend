use crate::{
	dot::{
		BalancesCall, Polkadot, PolkadotAccountId, PolkadotAccountIdLookup,
		PolkadotExtrinsicBuilder, PolkadotProxyType, PolkadotReplayProtection, PolkadotRuntimeCall,
		ProxyCall, UtilityCall,
	},
	FetchAssetParams, TransferAssetParams,
};
use sp_std::{boxed::Box, vec::Vec};

pub fn extrinsic_builder(
	replay_protection: PolkadotReplayProtection,
	fetch_params: Vec<FetchAssetParams<Polkadot>>,
	transfer_params: Vec<TransferAssetParams<Polkadot>>,
	vault_account: PolkadotAccountId,
) -> PolkadotExtrinsicBuilder {
	PolkadotExtrinsicBuilder::new(
		replay_protection,
		PolkadotRuntimeCall::Proxy(ProxyCall::proxy {
			real: PolkadotAccountIdLookup::from(vault_account),
			force_proxy_type: Some(PolkadotProxyType::Any),
			call: Box::new(PolkadotRuntimeCall::Utility(UtilityCall::batch {
				calls: [
					fetch_params
						.into_iter()
						.map(|fetch_param| {
							PolkadotRuntimeCall::Utility(UtilityCall::as_derivative {
								// TODO: refer to issue #2354
								index: fetch_param.deposit_fetch_id as u16,
								call: Box::new(PolkadotRuntimeCall::Balances(
									BalancesCall::transfer_all {
										dest: PolkadotAccountIdLookup::from(vault_account),
										keep_alive: false,
									},
								)),
							})
						})
						.collect::<Vec<PolkadotRuntimeCall>>(),
					transfer_params
						.into_iter()
						.map(|transfer_param| {
							PolkadotRuntimeCall::Balances(BalancesCall::transfer {
								dest: PolkadotAccountIdLookup::from(transfer_param.to),
								value: transfer_param.amount,
							})
						})
						.collect::<Vec<PolkadotRuntimeCall>>(),
				]
				.concat(),
			})),
		}),
	)
}

#[cfg(test)]
mod test_batch_fetch {

	use super::*;
	use crate::dot::{PolkadotPair, NONCE_1, RAW_SEED_1, RAW_SEED_2, TEST_RUNTIME_VERSION};
	use cf_primitives::chains::assets;

	#[test]
	fn create_test_api_call() {
		let keypair_vault = PolkadotPair::from_seed(&RAW_SEED_1);
		let account_id_vault = keypair_vault.public_key();

		let keypair_proxy = PolkadotPair::from_seed(&RAW_SEED_2);

		let dummy_fetch_params: Vec<FetchAssetParams<Polkadot>> = vec![
			FetchAssetParams::<Polkadot> { deposit_fetch_id: 1, asset: assets::dot::Asset::Dot },
			FetchAssetParams::<Polkadot> { deposit_fetch_id: 2, asset: assets::dot::Asset::Dot },
			FetchAssetParams::<Polkadot> { deposit_fetch_id: 3, asset: assets::dot::Asset::Dot },
		];

		let dummy_transfer_params: Vec<TransferAssetParams<Polkadot>> = vec![
			TransferAssetParams::<Polkadot> {
				to: PolkadotAccountId::from_alias_inner([7u8; 32]),
				amount: 4,
				asset: assets::dot::Asset::Dot,
			},
			TransferAssetParams::<Polkadot> {
				to: PolkadotAccountId::from_alias_inner([8u8; 32]),
				amount: 5,
				asset: assets::dot::Asset::Dot,
			},
			TransferAssetParams::<Polkadot> {
				to: PolkadotAccountId::from_alias_inner([9u8; 32]),
				amount: 6,
				asset: assets::dot::Asset::Dot,
			},
		];

		let mut builder = super::extrinsic_builder(
			PolkadotReplayProtection { nonce: NONCE_1, genesis_hash: Default::default() },
			dummy_fetch_params,
			dummy_transfer_params,
			account_id_vault,
		);

		let payload = builder.get_signature_payload(
			TEST_RUNTIME_VERSION.spec_version,
			TEST_RUNTIME_VERSION.transaction_version,
		);
		assert_eq!(
			hex::encode(&payload.0),
			"fefffc6f6999882f0481ac2a5c5df813b53adf448a478fb1420f89df84455df3"
		);
		builder.insert_signature(keypair_proxy.public_key(), keypair_proxy.sign(&payload));
		assert!(builder.is_signed());
	}
}
