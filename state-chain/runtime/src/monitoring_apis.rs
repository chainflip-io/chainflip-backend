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
	chainflip::Offence, AuctionOutcome, BTreeMap, DelegationSnapshot, DispatchErrorWithMessage,
	ValidatorInfo,
};
use cf_chains::{
	dot::PolkadotAccountId,
	sol::{api::DurableNonceAndAccount, SolAddress, SolSignature},
};
use cf_primitives::AssetAmount;
use codec::{Decode, Encode};
use frame_support::sp_runtime::AccountId32;
use pallet_cf_asset_balances::VaultImbalance;
use scale_info::{prelude::string::String, TypeInfo};
use serde::{Deserialize, Serialize};
use sp_api::decl_runtime_apis;
use sp_runtime::BoundedVec;
use sp_std::vec::Vec;

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct ExternalChainsBlockHeight {
	pub bitcoin: u64,
	pub ethereum: u64,
	pub polkadot: u64,
	pub solana: u64,
	pub arbitrum: u64,
	pub assethub: u64,
}
#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct BtcUtxos {
	pub total_balance: u64,
	pub count: u32,
}

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct EpochState {
	pub epoch_duration: u32,
	pub current_epoch_started_at: u32,
	pub current_epoch_index: u32,
	pub min_active_bid: Option<u128>,
	pub rotation_phase: String,
}

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct RedemptionsInfo {
	pub total_balance: u128,
	pub count: u32,
}
#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct PendingBroadcasts {
	pub ethereum: u32,
	pub bitcoin: u32,
	pub polkadot: u32,
	pub arbitrum: u32,
	pub solana: u32,
	pub assethub: u32,
}
#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct PendingTssCeremonies {
	pub evm: u32,
	pub bitcoin: u32,
	pub polkadot: u32,
	pub solana: u32,
}
#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct OpenDepositChannels {
	pub ethereum: u32,
	pub bitcoin: u32,
	pub polkadot: u32,
	pub arbitrum: u32,
	pub solana: u32,
	pub assethub: u32,
}
#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct FeeImbalance<A> {
	pub ethereum: VaultImbalance<A>,
	pub polkadot: VaultImbalance<A>,
	pub arbitrum: VaultImbalance<A>,
	pub bitcoin: VaultImbalance<A>,
	pub solana: VaultImbalance<A>,
	pub assethub: VaultImbalance<A>,
}

impl<A> FeeImbalance<A> {
	pub fn map<B>(&self, f: impl Fn(&A) -> B) -> FeeImbalance<B> {
		FeeImbalance {
			ethereum: self.ethereum.map(&f),
			polkadot: self.polkadot.map(&f),
			arbitrum: self.arbitrum.map(&f),
			bitcoin: self.bitcoin.map(&f),
			solana: self.solana.map(&f),
			assethub: self.assethub.map(&f),
		}
	}
}

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct AuthoritiesInfo {
	pub authorities: u32,
	pub online_authorities: u32,
	#[deprecated]
	pub backups: u32,
	#[deprecated]
	pub online_backups: u32,
}

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct LastRuntimeUpgradeInfo {
	pub spec_version: u32,
	pub spec_name: sp_runtime::RuntimeString,
}

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct FlipSupply {
	pub total_supply: u128,
	pub offchain_supply: u128,
}

#[derive(
	Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone, Default,
)]
pub struct SolanaNonces {
	pub available: Vec<DurableNonceAndAccount>,
	pub unavailable: Vec<SolAddress>,
}

#[derive(
	Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone, Default,
)]
pub struct ActivateKeysBroadcastIds {
	pub ethereum: Option<u32>,
	pub bitcoin: Option<u32>,
	pub polkadot: Option<u32>,
	pub arbitrum: Option<u32>,
	pub solana: (Option<u32>, Option<SolSignature>),
	pub assethub: Option<u32>,
}

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct MonitoringDataV2 {
	pub external_chains_height: ExternalChainsBlockHeight,
	pub btc_utxos: BtcUtxos,
	pub epoch: EpochState,
	pub pending_redemptions: RedemptionsInfo,
	pub pending_broadcasts: PendingBroadcasts,
	pub pending_tss: PendingTssCeremonies,
	pub open_deposit_channels: OpenDepositChannels,
	pub fee_imbalance: FeeImbalance<AssetAmount>,
	pub authorities: AuthoritiesInfo,
	pub build_version: LastRuntimeUpgradeInfo,
	pub suspended_validators: Vec<(Offence, u32)>,
	pub pending_swaps: u32,
	pub dot_aggkey: PolkadotAccountId,
	pub flip_supply: FlipSupply,
	pub sol_aggkey: SolAddress,
	pub sol_onchain_key: SolAddress,
	pub sol_nonces: SolanaNonces,
	pub activating_key_broadcast_ids: ActivateKeysBroadcastIds,
}

decl_runtime_apis!(
	#[api_version(2)]
	pub trait MonitoringRuntimeApi {
		fn cf_authorities() -> AuthoritiesInfo;
		fn cf_external_chains_block_height() -> ExternalChainsBlockHeight;
		fn cf_btc_utxos() -> BtcUtxos;
		fn cf_dot_aggkey() -> PolkadotAccountId;
		fn cf_suspended_validators() -> Vec<(Offence, u32)>;
		fn cf_epoch_state() -> EpochState;
		fn cf_redemptions() -> RedemptionsInfo;
		fn cf_pending_broadcasts_count() -> PendingBroadcasts;
		fn cf_pending_tss_ceremonies_count() -> PendingTssCeremonies;
		fn cf_pending_swaps_count() -> u32;
		fn cf_open_deposit_channels_count() -> OpenDepositChannels;
		fn cf_fee_imbalance() -> FeeImbalance<AssetAmount>;
		fn cf_build_version() -> LastRuntimeUpgradeInfo;
		fn cf_rotation_broadcast_ids() -> ActivateKeysBroadcastIds;
		fn cf_sol_nonces() -> SolanaNonces;
		fn cf_sol_aggkey() -> SolAddress;
		fn cf_sol_onchain_key() -> SolAddress;
		fn cf_monitoring_data() -> MonitoringDataV2;
		fn cf_accounts_info(
			accounts: BoundedVec<AccountId32, sp_core::ConstU32<10>>,
		) -> Vec<ValidatorInfo>;
		#[allow(clippy::type_complexity)]
		fn cf_simulate_auction() -> Result<
			(
				AuctionOutcome<AccountId32, AssetAmount>,
				BTreeMap<AccountId32, DelegationSnapshot<AccountId32, AssetAmount>>,
				Vec<AccountId32>,
				AssetAmount,
			),
			DispatchErrorWithMessage,
		>;
	}
);
