use crate::{
	dot::{
		PolkadotAccountId, PolkadotAccountIdLookup, PolkadotProxyType, PolkadotReplayProtection,
	},
	hub::{
		Assethub, AssethubExtrinsicBuilder, AssethubRuntimeCall, AssetsCall, BalancesCall,
		ProxyCall, UtilityCall,
	},
	FetchAssetParams, TransferAssetParams,
};
use cf_primitives::{ASSETHUB_USDC_ASSET_ID, ASSETHUB_USDT_ASSET_ID};
use cf_utilities::SliceToArray;
use sp_std::{boxed::Box, vec::Vec};

pub fn extrinsic_builder(
	replay_protection: PolkadotReplayProtection,
	fetch_params: Vec<FetchAssetParams<Assethub>>,
	transfer_params: Vec<TransferAssetParams<Assethub>>,
	vault_account: PolkadotAccountId,
) -> AssethubExtrinsicBuilder {
	AssethubExtrinsicBuilder::new(
		replay_protection,
		AssethubRuntimeCall::Proxy(ProxyCall::proxy {
			real: PolkadotAccountIdLookup::from(vault_account),
			force_proxy_type: Some(PolkadotProxyType::Any),
			call: Box::new(AssethubRuntimeCall::Utility(UtilityCall::force_batch {
				calls: [
					fetch_params
						.into_iter()
						.map(|fetch_param| utility_fetch(fetch_param, vault_account))
						.collect::<Vec<AssethubRuntimeCall>>(),
					transfer_params
						.into_iter()
						.map(|transfer_param| match transfer_param.asset {
							cf_primitives::chains::assets::hub::Asset::HubDot =>
								AssethubRuntimeCall::Balances(BalancesCall::transfer_allow_death {
									dest: PolkadotAccountIdLookup::from(transfer_param.to),
									value: transfer_param.amount,
								}),
							cf_primitives::chains::assets::hub::Asset::HubUsdt =>
								AssethubRuntimeCall::Assets(AssetsCall::transfer {
									id: ASSETHUB_USDT_ASSET_ID,
									dest: PolkadotAccountIdLookup::from(transfer_param.to),
									value: transfer_param.amount,
								}),
							cf_primitives::chains::assets::hub::Asset::HubUsdc =>
								AssethubRuntimeCall::Assets(AssetsCall::transfer {
									id: ASSETHUB_USDC_ASSET_ID,
									dest: PolkadotAccountIdLookup::from(transfer_param.to),
									value: transfer_param.amount,
								}),
						})
						.collect::<Vec<AssethubRuntimeCall>>(),
				]
				.concat(),
			})),
		}),
	)
}

fn utility_fetch(
	fetch_param: FetchAssetParams<Assethub>,
	vault_account: PolkadotAccountId,
) -> AssethubRuntimeCall {
	let layers = fetch_param
		.deposit_fetch_id
		.to_be_bytes()
		.chunks(2)
		.map(|chunk| u16::from_be_bytes(chunk.copy_to_array::<2>()))
		.skip_while(|layer| *layer == 0u16)
		.collect::<Vec<u16>>();

	layers.into_iter().fold(
		match fetch_param.asset {
			cf_primitives::chains::assets::hub::Asset::HubDot =>
				AssethubRuntimeCall::Balances(BalancesCall::transfer_all {
					dest: PolkadotAccountIdLookup::from(vault_account),
					keep_alive: false,
				}),
			cf_primitives::chains::assets::hub::Asset::HubUsdt =>
				AssethubRuntimeCall::Assets(AssetsCall::transfer {
					id: ASSETHUB_USDT_ASSET_ID,
					dest: PolkadotAccountIdLookup::from(vault_account),
					value: fetch_param.amount,
				}),
			cf_primitives::chains::assets::hub::Asset::HubUsdc =>
				AssethubRuntimeCall::Assets(AssetsCall::transfer {
					id: ASSETHUB_USDC_ASSET_ID,
					dest: PolkadotAccountIdLookup::from(vault_account),
					value: fetch_param.amount,
				}),
		},
		|call, index| {
			AssethubRuntimeCall::Utility(UtilityCall::as_derivative { index, call: Box::new(call) })
		},
	)
}

#[cfg(test)]
mod test_batch_fetch {

	use super::*;
	use crate::{
		dot::{PolkadotPair, NONCE_1, RAW_SEED_1, RAW_SEED_2},
		hub::TEST_RUNTIME_VERSION,
	};
	use cf_primitives::chains::assets;

	#[test]
	fn create_test_api_call() {
		let keypair_vault = PolkadotPair::from_seed(&RAW_SEED_1);
		let account_id_vault = keypair_vault.public_key();

		let keypair_proxy = PolkadotPair::from_seed(&RAW_SEED_2);

		let dummy_fetch_params: Vec<FetchAssetParams<Assethub>> = vec![
			FetchAssetParams::<Assethub> {
				deposit_fetch_id: 1,
				asset: assets::hub::Asset::HubDot,
				amount: 44,
			},
			FetchAssetParams::<Assethub> {
				deposit_fetch_id: 2,
				asset: assets::hub::Asset::HubUsdc,
				amount: 55,
			},
			FetchAssetParams::<Assethub> {
				deposit_fetch_id: 3,
				asset: assets::hub::Asset::HubUsdt,
				amount: 66,
			},
		];

		let dummy_transfer_params: Vec<TransferAssetParams<Assethub>> = vec![
			TransferAssetParams::<Assethub> {
				to: PolkadotAccountId::from_aliased([7u8; 32]),
				amount: 4,
				asset: assets::hub::Asset::HubDot,
			},
			TransferAssetParams::<Assethub> {
				to: PolkadotAccountId::from_aliased([8u8; 32]),
				amount: 5,
				asset: assets::hub::Asset::HubUsdc,
			},
			TransferAssetParams::<Assethub> {
				to: PolkadotAccountId::from_aliased([9u8; 32]),
				amount: 6,
				asset: assets::hub::Asset::HubUsdt,
			},
		];

		let mut builder: AssethubExtrinsicBuilder = super::extrinsic_builder(
			PolkadotReplayProtection {
				nonce: NONCE_1,
				signer: keypair_proxy.public_key(),
				genesis_hash: hex_literal::hex!(
					"68d56f15f85d3136970ec16946040bc1752654e906147f7e43e9d539d7c3de2f"
				)
				.into(),
			},
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
			"050486994422289de8a869459a13fb7b3c7af8a1de45c1bcf7c5d805e6ea9721"
		);
		builder
			.insert_signer_and_signature(keypair_proxy.public_key(), keypair_proxy.sign(&payload));
		assert!(builder.is_signed());
	}

	#[test]
	fn nested_fetch() {
		let fetch_param = FetchAssetParams::<Assethub> {
			deposit_fetch_id: 0x0004_0003_0002_0001,
			asset: assets::hub::Asset::HubDot,
			amount: 123456,
		};
		let vault_account = PolkadotAccountId::from_aliased([1u8; 32]);
		let call = utility_fetch(fetch_param, vault_account);

		assert_eq!(
			call,
			AssethubRuntimeCall::Utility(UtilityCall::as_derivative {
				index: 0x0001,
				call: Box::new(AssethubRuntimeCall::Utility(UtilityCall::as_derivative {
					index: 0x0002,
					call: Box::new(AssethubRuntimeCall::Utility(UtilityCall::as_derivative {
						index: 0x0003,
						call: Box::new(AssethubRuntimeCall::Utility(UtilityCall::as_derivative {
							index: 0x0004,
							call: Box::new(AssethubRuntimeCall::Balances(
								BalancesCall::transfer_all {
									dest: PolkadotAccountIdLookup::from(vault_account),
									keep_alive: false,
								}
							)),
						})),
					})),
				})),
			})
		);

		let fetch_param = FetchAssetParams::<Assethub> {
			deposit_fetch_id: 1,
			asset: assets::hub::Asset::HubUsdc,
			amount: 123456,
		};
		let vault_account = PolkadotAccountId::from_aliased([1u8; 32]);
		let call = utility_fetch(fetch_param, vault_account);

		assert_eq!(
			call,
			AssethubRuntimeCall::Utility(UtilityCall::as_derivative {
				index: 1,
				call: Box::new(AssethubRuntimeCall::Assets(AssetsCall::transfer {
					dest: PolkadotAccountIdLookup::from(vault_account),
					id: ASSETHUB_USDC_ASSET_ID,
					value: 123456
				})),
			})
		);
	}

	#[test]
	fn fetch_equivalence() {
		let fetch_param_1 = FetchAssetParams::<Assethub> {
			deposit_fetch_id: 0x0000_0000_0000_0001,
			asset: assets::hub::Asset::HubDot,
			amount: 123456,
		};
		let fetch_param_2 = FetchAssetParams::<Assethub> {
			deposit_fetch_id: 0x0000_0000_0001_0000,
			asset: assets::hub::Asset::HubDot,
			amount: 123456,
		};
		let vault_account = PolkadotAccountId::from_aliased([1u8; 32]);
		let call_1 = utility_fetch(fetch_param_1, vault_account);
		let call_2 = utility_fetch(fetch_param_2, vault_account);

		assert_ne!(call_1, call_2);
	}
}
