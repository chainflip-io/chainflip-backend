use crate::{
	dot::{
		PolkadotAccountId, PolkadotAccountIdLookup, PolkadotProxyType, PolkadotReplayProtection,
	},
	hub::{
		Assethub, AssethubExtrinsicBuilder, AssethubRuntimeCall, BalancesCall, ProxyCall,
		UtilityCall,
	},
	FetchAssetParams, TransferAssetParams,
};
use cf_primitives::ChannelId;
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
						.map(|fetch_param| {
							utility_fetch(fetch_param.deposit_fetch_id, vault_account)
						})
						.collect::<Vec<AssethubRuntimeCall>>(),
					transfer_params
						.into_iter()
						.map(|transfer_param| {
							AssethubRuntimeCall::Balances(BalancesCall::transfer_allow_death {
								dest: PolkadotAccountIdLookup::from(transfer_param.to),
								value: transfer_param.amount,
							})
						})
						.collect::<Vec<AssethubRuntimeCall>>(),
				]
				.concat(),
			})),
		}),
	)
}

fn utility_fetch(channel_id: ChannelId, vault_account: PolkadotAccountId) -> AssethubRuntimeCall {
	let layers = channel_id
		.to_be_bytes()
		.chunks(2)
		.map(|chunk| u16::from_be_bytes(chunk.as_array::<2>()))
		.skip_while(|layer| *layer == 0u16)
		.collect::<Vec<u16>>();

	layers.into_iter().fold(
		AssethubRuntimeCall::Balances(BalancesCall::transfer_all {
			dest: PolkadotAccountIdLookup::from(vault_account),
			keep_alive: false,
		}),
		|call, index| {
			AssethubRuntimeCall::Utility(UtilityCall::as_derivative { index, call: Box::new(call) })
		},
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

		let dummy_fetch_params: Vec<FetchAssetParams<Assethub>> = vec![
			FetchAssetParams::<Assethub> { deposit_fetch_id: 1, asset: assets::hub::Asset::HubDot },
			FetchAssetParams::<Assethub> { deposit_fetch_id: 2, asset: assets::hub::Asset::HubDot },
			FetchAssetParams::<Assethub> { deposit_fetch_id: 3, asset: assets::hub::Asset::HubDot },
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
				asset: assets::hub::Asset::HubDot,
			},
			TransferAssetParams::<Assethub> {
				to: PolkadotAccountId::from_aliased([9u8; 32]),
				amount: 6,
				asset: assets::hub::Asset::HubDot,
			},
		];

		let mut builder = super::extrinsic_builder(
			PolkadotReplayProtection {
				nonce: NONCE_1,
				signer: keypair_proxy.public_key(),
				genesis_hash: Default::default(),
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
			"fedd552924ecdbf18c13d7f534b344926b2a771a03a59d095af0676f98f6d19e"
		);
		builder
			.insert_signer_and_signature(keypair_proxy.public_key(), keypair_proxy.sign(&payload));
		assert!(builder.is_signed());
	}

	#[test]
	fn nested_fetch() {
		let channel_id = 0x0004_0003_0002_0001;
		let vault_account = PolkadotAccountId::from_aliased([1u8; 32]);
		let call = utility_fetch(channel_id, vault_account);

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

		let channel_id = 1;
		let vault_account = PolkadotAccountId::from_aliased([1u8; 32]);
		let call = utility_fetch(channel_id, vault_account);

		assert_eq!(
			call,
			AssethubRuntimeCall::Utility(UtilityCall::as_derivative {
				index: 1,
				call: Box::new(AssethubRuntimeCall::Balances(BalancesCall::transfer_all {
					dest: PolkadotAccountIdLookup::from(vault_account),
					keep_alive: false,
				})),
			})
		);
	}

	#[test]
	fn fetch_equivalence() {
		let channel_id_1 = 0x0000_0000_0000_0001;
		let channel_id_2 = 0x0000_0000_0001_0000;
		let vault_account = PolkadotAccountId::from_aliased([1u8; 32]);
		let call_1 = utility_fetch(channel_id_1, vault_account);
		let call_2 = utility_fetch(channel_id_2, vault_account);

		assert_ne!(call_1, call_2);
	}
}