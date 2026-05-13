// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use super::hub::calculate_derived_address;
use crate::{
	dot::{
		PolkadotAccountId, PolkadotAccountIdLookup, PolkadotProxyType, PolkadotReplayProtection,
	},
	hub::{
		as_derivative_u64, Assethub, AssethubExtrinsicBuilder, AssethubRuntimeCall, AssetsCall,
		BalancesCall, ProxyCall, UtilityCall, XcmCall,
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
	xcm_call: XcmCall,
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
					as_derivative_u64(
						derivative_index,
						AssethubRuntimeCall::Utility(UtilityCall::force_batch {
							calls: vec![
								AssethubRuntimeCall::PolkadotXcm(xcm_call),
								match transfer_params.asset {
									cf_primitives::chains::assets::hub::Asset::HubDot =>
										AssethubRuntimeCall::Balances(BalancesCall::transfer_all {
											dest: PolkadotAccountIdLookup::from(transfer_params.to),
											keep_alive: false,
										}),
									cf_primitives::chains::assets::hub::Asset::HubUsdt =>
										AssethubRuntimeCall::Assets(AssetsCall::transfer_all {
											id: ASSETHUB_USDT_ASSET_ID,
											dest: PolkadotAccountIdLookup::from(transfer_params.to),
											keep_alive: false,
										}),
									cf_primitives::chains::assets::hub::Asset::HubUsdc =>
										AssethubRuntimeCall::Assets(AssetsCall::transfer_all {
											id: ASSETHUB_USDC_ASSET_ID,
											dest: PolkadotAccountIdLookup::from(transfer_params.to),
											keep_alive: false,
										}),
								},
							],
						}),
					),
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

		let mut data: &[u8] = hex_literal::hex!("0b03010003000101008eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a480304000000000700e87648170000000000").as_ref();
		let xcm_call = <XcmCall as codec::Decode>::decode(&mut data).unwrap();

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

	fn simulated_origin(
		vault: PolkadotAccountId,
		mut call: &AssethubRuntimeCall,
	) -> PolkadotAccountId {
		let mut origin = vault;
		while let AssethubRuntimeCall::Utility(UtilityCall::as_derivative { index, call: inner }) =
			call
		{
			origin = {
				PolkadotAccountId::from_aliased(crate::hub::calculate_derived_address_utility(
					*origin.aliased_ref(),
					*index,
				))
			};
			call = inner.as_ref();
		}
		origin
	}

	// For every derivative index, the account that `extrinsic_builder` funds must equal the
	// account the runtime would execute the user call from. Walks the built `as_derivative`
	// chain in-process using the same `blake2_256("modlpy/utilisuba" || parent || index_le)`
	// that pallet-utility uses on Assethub, and compares against `calculate_derived_address`.
	//
	// Uniform `any::<u64>()` sampling would rarely hit the small-id boundaries where the
	// original truncation bug lived, so `prop_oneof!` biases the strategy toward single-,
	// two-, and three-layer ranges plus the specific cutover points.
	proptest::proptest! {
		#[test]
		fn funded_address_matches_execution_origin_for_all_ids(
			id in proptest::prop_oneof![
				1 => proptest::prelude::Just(0u64),
				1 => proptest::prelude::Just(1u64),
				1 => proptest::prelude::Just(0xFFFFu64),
				1 => proptest::prelude::Just(0x0001_0000u64),
				1 => proptest::prelude::Just(u64::MAX),
				3 => 0u64..=0xFFFFu64,
				3 => 0x0001_0000u64..=0x000F_FFFFu64,
				2 => 0x0001_0000_0000u64..=0x000F_FFFF_FFFFu64,
				4 => proptest::prelude::any::<u64>(),
			],
		) {
			use crate::hub::xcm_types::hub_runtime_types::xcm::*;
			use crate::hub::xcm_types::hub_runtime_types::staging_xcm::v5::location::*;
			use crate::hub::xcm_types::hub_runtime_types::staging_xcm::v5::junctions::*;
			use crate::hub::xcm_types::hub_runtime_types::staging_xcm::v5::asset::*;
			use crate::hub::as_derivative_u64;
			use proptest::{prop_assert_eq, prop_assert_ne};

			let vault = PolkadotAccountId::from_aliased([1u8; 32]);
			let recipient = PolkadotAccountId::from_aliased([2u8; 32]);
			let dummy_user_call = XcmCall::teleport_assets {
				dest: VersionedLocation::V5(
					Location {
						parents: 0,
						interior: Junctions::Here,
					}
				),
				beneficiary: VersionedLocation::V5(
					Location {
						parents: 0,
						interior: Junctions::Here,
					}
				),
				assets: VersionedAssets::V5(Assets(Default::default())),
				fee_asset_item: 0
			};

			let builder = super::extrinsic_builder(
				PolkadotReplayProtection {
					nonce: NONCE_1,
					signer: PolkadotAccountId::from_aliased([9u8; 32]),
					genesis_hash: Default::default(),
				},
				id,
				TransferAssetParams::<Assethub> {
					to: recipient,
					amount: 1_000,
					asset: assets::hub::Asset::HubDot,
				},
				vault,
				dummy_user_call.clone(),
			);

			let outer_batch = match &builder.extrinsic_call {
				AssethubRuntimeCall::Proxy(ProxyCall::proxy { call, .. }) => match call.as_ref() {
					AssethubRuntimeCall::Utility(UtilityCall::force_batch { calls }) => calls,
					other => panic!("unexpected inner of proxy: {other:?}"),
				},
				other => panic!("unexpected outer call: {other:?}"),
			};
			prop_assert_eq!(outer_batch.len(), 2);

			let funded_dest = match &outer_batch[0] {
				AssethubRuntimeCall::Balances(BalancesCall::transfer_allow_death {
					dest, ..
				}) => dest.clone(),
				other => panic!("expected funding transfer, got {other:?}"),
			};

			let execution_origin = simulated_origin(vault, &outer_batch[1]);

			prop_assert_eq!(
				PolkadotAccountIdLookup::from(execution_origin),
				funded_dest,
				"funded destination and execution origin disagree for id {:#018x}", id,
			);
			prop_assert_eq!(
				execution_origin,
				crate::hub::calculate_derived_address(vault, id),
				"execution origin doesn't match calculate_derived_address for id {:#018x}", id,
			);
			prop_assert_ne!(execution_origin, vault,
				"execution origin should never be the vault, id {:#018x}", id);

			// Sanity: the same id fed through as_derivative_u64 in isolation agrees too.
			let standalone = as_derivative_u64(id, AssethubRuntimeCall::PolkadotXcm(dummy_user_call.clone()));
			prop_assert_eq!(simulated_origin(vault, &standalone), execution_origin);
		}
	}
}
