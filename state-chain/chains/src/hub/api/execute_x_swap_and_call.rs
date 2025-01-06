use super::hub::calculate_derived_address;
use crate::{
	dot::{
		PolkadotAccountId, PolkadotAccountIdLookup, PolkadotProxyType, PolkadotReplayProtection,
	},
	hub::{
		Assethub, AssethubExtrinsicBuilder, AssethubRuntimeCall, AssetsCall, BalancesCall,
		ProxyCall, UtilityCall,
	},
	TransferAssetParams,
};
use cf_primitives::{ChannelId, ASSETHUB_USDC_ASSET_ID, ASSETHUB_USDT_ASSET_ID};
use sp_std::{boxed::Box, vec};

pub fn extrinsic_builder(
	replay_protection: PolkadotReplayProtection,
	derivative_index: ChannelId,
	transfer_params: TransferAssetParams<Assethub>,
	vault_account: PolkadotAccountId,
	xcm_call: AssethubRuntimeCall,
) -> AssethubExtrinsicBuilder {
	let derived_address = calculate_derived_address(vault_account, derivative_index);

	AssethubExtrinsicBuilder::new(
		replay_protection,
		AssethubRuntimeCall::Proxy(ProxyCall::proxy {
			real: PolkadotAccountIdLookup::from(vault_account),
			force_proxy_type: Some(PolkadotProxyType::Any),
			call: Box::new(AssethubRuntimeCall::Utility(UtilityCall::force_batch {
				calls: vec![
					match transfer_params.asset {
						cf_primitives::chains::assets::hub::Asset::HubDot =>
							AssethubRuntimeCall::Balances(BalancesCall::transfer_allow_death {
								dest: PolkadotAccountIdLookup::from(derived_address),
								value: transfer_params.amount,
							}),
						cf_primitives::chains::assets::hub::Asset::HubUsdt =>
							AssethubRuntimeCall::Assets(AssetsCall::transfer {
								id: ASSETHUB_USDT_ASSET_ID,
								dest: PolkadotAccountIdLookup::from(derived_address),
								value: transfer_params.amount,
							}),
						cf_primitives::chains::assets::hub::Asset::HubUsdc =>
							AssethubRuntimeCall::Assets(AssetsCall::transfer {
								id: ASSETHUB_USDC_ASSET_ID,
								dest: PolkadotAccountIdLookup::from(derived_address),
								value: transfer_params.amount,
							}),
					},
					AssethubRuntimeCall::Utility(UtilityCall::as_derivative {
						index: derivative_index as u16,
						call: Box::new(AssethubRuntimeCall::Utility(UtilityCall::force_batch {
							calls: vec![
								xcm_call,
								match transfer_params.asset {
									cf_primitives::chains::assets::hub::Asset::HubDot =>
										AssethubRuntimeCall::Balances(BalancesCall::transfer_all {
											dest: PolkadotAccountIdLookup::from(transfer_params.to),
											keep_alive: false,
										}),
									cf_primitives::chains::assets::hub::Asset::HubUsdt =>
										AssethubRuntimeCall::Assets(AssetsCall::transfer {
											id: ASSETHUB_USDT_ASSET_ID,
											dest: PolkadotAccountIdLookup::from(transfer_params.to),
											value: transfer_params.amount,
										}),
									cf_primitives::chains::assets::hub::Asset::HubUsdc =>
										AssethubRuntimeCall::Assets(AssetsCall::transfer {
											id: ASSETHUB_USDC_ASSET_ID,
											dest: PolkadotAccountIdLookup::from(transfer_params.to),
											value: transfer_params.amount,
										}),
								},
							],
						})),
					}),
				],
			})),
		}),
	)
}

#[cfg(test)]
mod test_xcm_call {

	use super::*;
	use crate::{
		dot::{PolkadotPair, NONCE_1, RAW_SEED_1, RAW_SEED_2},
		hub::TEST_RUNTIME_VERSION,
	};
	use cf_primitives::chains::assets;

	#[test]
	fn create_test_call() {
		let keypair_vault = PolkadotPair::from_seed(&RAW_SEED_1);
		let account_id_vault = keypair_vault.public_key();

		let keypair_proxy = PolkadotPair::from_seed(&RAW_SEED_2);

		let transfer_params = TransferAssetParams::<Assethub> {
			to: PolkadotAccountId::default(),
			amount: 40000000000,
			asset: assets::hub::Asset::HubDot,
		};

		let mut data: &[u8] = hex_literal::hex!("1f0b03010003000101008eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a480304000000000700e87648170000000000").as_ref();
		let xcm_call = <AssethubRuntimeCall as codec::Decode>::decode(&mut data).unwrap();

		let mut builder: AssethubExtrinsicBuilder = super::extrinsic_builder(
			PolkadotReplayProtection {
				nonce: NONCE_1,
				signer: keypair_proxy.public_key(),
				genesis_hash: hex_literal::hex!(
					"68d56f15f85d3136970ec16946040bc1752654e906147f7e43e9d539d7c3de2f"
				)
				.into(),
			},
			1337,
			transfer_params,
			account_id_vault,
			xcm_call,
		);

		let payload = builder.get_signature_payload(
			TEST_RUNTIME_VERSION.spec_version,
			TEST_RUNTIME_VERSION.transaction_version,
		);
		assert_eq!(
			hex::encode(&payload.0),
			"d2293ae3f37af28d879d098307b12aa8e672a0a28c5450297e999d6b026757ca"
		);
		builder
			.insert_signer_and_signature(keypair_proxy.public_key(), keypair_proxy.sign(&payload));
		assert!(builder.is_signed());
	}
}
