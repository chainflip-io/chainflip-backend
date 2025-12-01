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

pub mod types;

use super::types::*;
pub use pallet_cf_validator::AuctionOutcome;
use sp_api::decl_runtime_apis;
use types::*;

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
			accounts: BoundedVec<AccountId, sp_core::ConstU32<10>>,
		) -> Vec<ValidatorInfo>;
		#[allow(clippy::allow_attributes)]
		#[allow(clippy::type_complexity)]
		fn cf_simulate_auction() -> Result<
			(
				AuctionOutcome<AccountId, AssetAmount>,
				BTreeMap<AccountId, DelegationSnapshot<AccountId, AssetAmount>>,
				Vec<AccountId>,
				AssetAmount,
			),
			DispatchErrorWithMessage,
		>;
	}
);
