use crate::{
	dot::{
		BalancesCall, Polkadot, PolkadotAccountId, PolkadotAccountIdLookup,
		PolkadotExtrinsicBuilder, PolkadotProxyType, PolkadotReplayProtection, PolkadotRuntimeCall,
		ProxyCall, UtilityCall,
	},
	FetchAssetParams, TransferAssetParams,
};
use cf_primitives::ChannelId;
use cf_utilities::SliceToArray;
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
			call: Box::new(PolkadotRuntimeCall::Utility(UtilityCall::force_batch {
				calls: [
					fetch_params
						.into_iter()
						.map(|fetch_param| {
							utility_fetch(fetch_param.deposit_fetch_id, vault_account)
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

fn utility_fetch(channel_id: ChannelId, vault_account: PolkadotAccountId) -> PolkadotRuntimeCall {
	let mut layers = channel_id
		.to_be_bytes()
		.chunks(2)
		.map(|chunk| u16::from_be_bytes(chunk.as_array::<2>()))
		.skip_while(|layer| *layer == 0u16)
		.collect::<Vec<u16>>();

	layers.reverse();
	layers.into_iter().fold(
		PolkadotRuntimeCall::Balances(BalancesCall::transfer_all {
			dest: PolkadotAccountIdLookup::from(vault_account),
			keep_alive: false,
		}),
		|call, index| {
			PolkadotRuntimeCall::Utility(UtilityCall::as_derivative { index, call: Box::new(call) })
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

		let dummy_fetch_params: Vec<FetchAssetParams<Polkadot>> = vec![
			FetchAssetParams::<Polkadot> { deposit_fetch_id: 1, asset: assets::dot::Asset::Dot },
			FetchAssetParams::<Polkadot> { deposit_fetch_id: 2, asset: assets::dot::Asset::Dot },
			FetchAssetParams::<Polkadot> { deposit_fetch_id: 3, asset: assets::dot::Asset::Dot },
		];

		let dummy_transfer_params: Vec<TransferAssetParams<Polkadot>> = vec![
			TransferAssetParams::<Polkadot> {
				to: PolkadotAccountId::from_aliased([7u8; 32]),
				amount: 4,
				asset: assets::dot::Asset::Dot,
			},
			TransferAssetParams::<Polkadot> {
				to: PolkadotAccountId::from_aliased([8u8; 32]),
				amount: 5,
				asset: assets::dot::Asset::Dot,
			},
			TransferAssetParams::<Polkadot> {
				to: PolkadotAccountId::from_aliased([9u8; 32]),
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
			"6fdbf2de25ba016e2c8b4f8238d057066a6ea2a63770073c3b6dcee86b02aeff"
		);
		builder.insert_signature(keypair_proxy.public_key(), keypair_proxy.sign(&payload));
		assert!(builder.is_signed());
	}

	#[test]
	fn nested_fetch() {
		let channel_id = 0x0102_0304_0506_0708u64;
		let vault_account = PolkadotAccountId::from_aliased([1u8; 32]);
		let call = utility_fetch(channel_id, vault_account);

		assert_eq!(
			call,
			PolkadotRuntimeCall::Utility(UtilityCall::as_derivative {
				index: 0x0102,
				call: Box::new(PolkadotRuntimeCall::Utility(UtilityCall::as_derivative {
					index: 0x0304,
					call: Box::new(PolkadotRuntimeCall::Utility(UtilityCall::as_derivative {
						index: 0x0506,
						call: Box::new(PolkadotRuntimeCall::Utility(UtilityCall::as_derivative {
							index: 0x0708,
							call: Box::new(PolkadotRuntimeCall::Balances(
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
			PolkadotRuntimeCall::Utility(UtilityCall::as_derivative {
				index: 1,
				call: Box::new(PolkadotRuntimeCall::Balances(BalancesCall::transfer_all {
					dest: PolkadotAccountIdLookup::from(vault_account),
					keep_alive: false,
				})),
			})
		);
	}
}
