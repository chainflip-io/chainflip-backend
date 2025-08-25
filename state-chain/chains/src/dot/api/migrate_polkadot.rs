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

use crate::{
	dot::{
		PolkadotAccountId, PolkadotAccountIdLookup, PolkadotExtrinsicBuilder, PolkadotProxyType,
		PolkadotReplayProtection, PolkadotRuntimeCall, ProxyCall,
	},
	hub::xcm_types::hub_runtime_types::staging_xcm,
};
use sp_std::boxed::Box;

pub fn extrinsic_builder(
	amount: crate::dot::PolkadotBalance,
	replay_protection: PolkadotReplayProtection,
	from_polkadot_vault_account: PolkadotAccountId,
	to_assethub_vault_account: PolkadotAccountId,
) -> PolkadotExtrinsicBuilder {
	use crate::hub::xcm_types::hub_runtime_types;
	use hub_runtime_types::xcm::VersionedLocation;
	PolkadotExtrinsicBuilder::new(
		replay_protection,
		PolkadotRuntimeCall::Proxy(ProxyCall::proxy {
			real: PolkadotAccountIdLookup::from(from_polkadot_vault_account),
			force_proxy_type: Some(PolkadotProxyType::Any),
			call: Box::new(PolkadotRuntimeCall::Xcm(Box::new(
				crate::dot::XcmCall::limited_teleport_assets {
					dest: VersionedLocation::V4(
                        hub_runtime_types::staging_xcm::v4::location::Location {
                            parents: 0,
                            interior: hub_runtime_types::staging_xcm::v4::junctions::Junctions::X1([
                                hub_runtime_types::staging_xcm::v4::junction::Junction::Parachain(1000)
                            ])
                        }
                    ),
					beneficiary: VersionedLocation::V4(
                        hub_runtime_types::staging_xcm::v4::location::Location {
                            parents: 0,
                            interior: hub_runtime_types::staging_xcm::v4::junctions::Junctions::X1([
                                hub_runtime_types::staging_xcm::v4::junction::Junction::AccountId32 {
                                    network: None,
                                    id: to_assethub_vault_account.0
                                }
                            ])
                        }
                    ),
					assets: hub_runtime_types::xcm::VersionedAssets::V4(
                        staging_xcm::v4::asset::Assets(
                            sp_std::vec![
                                staging_xcm::v4::asset::Asset {
                                    fun: staging_xcm::v4::asset::Fungibility::Fungible(amount),
                                    id: staging_xcm::v4::asset::AssetId(
                                        hub_runtime_types::staging_xcm::v4::location::Location {
                                            parents: 0,
                                            interior: hub_runtime_types::staging_xcm::v4::junctions::Junctions::Here
                                        }
                                    )
                                }
                            ]
                        )

                    ),
					fee_asset_itme: 0,
					weight_limit: hub_runtime_types::xcm::v3::WeightLimit::Unlimited,
				},
			))),
		}),
	)
}
