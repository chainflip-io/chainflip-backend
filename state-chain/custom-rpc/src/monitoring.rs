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

use super::pass_through;
use crate::{BTreeMap, BlockT, CustomRpc, RpcAccountInfoV2, RpcResult};
use cf_chains::{dot::PolkadotAccountId, sol::SolAddress};
use cf_utilities::rpc::NumberOrHex;
use jsonrpsee::proc_macros::rpc;
use pallet_cf_validator::{AuctionOutcome, DelegationSnapshot};
use sc_client_api::{BlockchainEvents, HeaderBackend};
use serde::{Deserialize, Serialize};
use sp_core::{bounded_vec::BoundedVec, ConstU32};
use state_chain_runtime::{
	chainflip::Offence,
	runtime_apis::{
		monitoring_api::MonitoringRuntimeApi,
		types::{
			ActivateKeysBroadcastIds, AuthoritiesInfo, BtcUtxos, EpochState,
			ExternalChainsBlockHeight, FeeImbalance, FlipSupply, LastRuntimeUpgradeInfo,
			MonitoringDataV2, OpenDepositChannels, PendingBroadcasts, PendingTssCeremonies,
			RedemptionsInfo, SolanaNonces,
		},
	},
};

impl From<EpochState> for RpcEpochState {
	fn from(rotation_state: EpochState) -> Self {
		Self {
			epoch_duration: rotation_state.epoch_duration,
			current_epoch_started_at: rotation_state.current_epoch_started_at,
			current_epoch_index: rotation_state.current_epoch_index,
			rotation_phase: rotation_state.rotation_phase,
			min_active_bid: rotation_state.min_active_bid.map(Into::into),
		}
	}
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RpcRedemptionsInfo {
	pub total_balance: NumberOrHex,
	pub count: u32,
}
impl From<RedemptionsInfo> for RpcRedemptionsInfo {
	fn from(redemption_info: RedemptionsInfo) -> Self {
		Self { total_balance: redemption_info.total_balance.into(), count: redemption_info.count }
	}
}

pub type RpcFeeImbalance = FeeImbalance<NumberOrHex>;

#[derive(Serialize, Deserialize, Clone)]
pub struct RpcFlipSupply {
	pub total_supply: NumberOrHex,
	pub offchain_supply: NumberOrHex,
}
impl From<FlipSupply> for RpcFlipSupply {
	fn from(flip_supply: FlipSupply) -> Self {
		Self {
			total_supply: flip_supply.total_supply.into(),
			offchain_supply: flip_supply.offchain_supply.into(),
		}
	}
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RpcMonitoringData {
	pub external_chains_height: ExternalChainsBlockHeight,
	pub btc_utxos: BtcUtxos,
	pub epoch: RpcEpochState,
	pub pending_redemptions: RpcRedemptionsInfo,
	pub pending_broadcasts: PendingBroadcasts,
	pub pending_tss: PendingTssCeremonies,
	pub open_deposit_channels: OpenDepositChannels,
	pub fee_imbalance: RpcFeeImbalance,
	pub authorities: AuthoritiesInfo,
	pub build_version: LastRuntimeUpgradeInfo,
	pub suspended_validators: Vec<(Offence, u32)>,
	pub pending_swaps: u32,
	pub dot_aggkey: PolkadotAccountId,
	pub flip_supply: RpcFlipSupply,
	pub sol_aggkey: SolAddress,
	pub sol_onchain_key: SolAddress,
	pub sol_nonces: SolanaNonces,
	pub activating_key_broadcast_ids: ActivateKeysBroadcastIds,
}
impl From<MonitoringDataV2> for RpcMonitoringData {
	fn from(monitoring_data: MonitoringDataV2) -> Self {
		Self {
			epoch: monitoring_data.epoch.into(),
			pending_redemptions: monitoring_data.pending_redemptions.into(),
			fee_imbalance: monitoring_data.fee_imbalance.map(|i| (*i).into()),
			external_chains_height: monitoring_data.external_chains_height,
			btc_utxos: monitoring_data.btc_utxos,
			pending_broadcasts: monitoring_data.pending_broadcasts,
			pending_tss: monitoring_data.pending_tss,
			open_deposit_channels: monitoring_data.open_deposit_channels,
			authorities: monitoring_data.authorities,
			build_version: monitoring_data.build_version,
			suspended_validators: monitoring_data.suspended_validators,
			pending_swaps: monitoring_data.pending_swaps,
			dot_aggkey: monitoring_data.dot_aggkey,
			flip_supply: monitoring_data.flip_supply.into(),
			sol_aggkey: monitoring_data.sol_aggkey,
			sol_onchain_key: monitoring_data.sol_onchain_key,
			sol_nonces: monitoring_data.sol_nonces,
			activating_key_broadcast_ids: monitoring_data.activating_key_broadcast_ids,
		}
	}
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RpcEpochState {
	pub epoch_duration: u32,
	pub current_epoch_started_at: u32,
	pub current_epoch_index: u32,
	pub min_active_bid: Option<NumberOrHex>,
	pub rotation_phase: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AuctionResult {
	auction_outcome: AuctionOutcome<state_chain_runtime::AccountId, NumberOrHex>,
	operators_info: BTreeMap<
		state_chain_runtime::AccountId,
		DelegationSnapshot<state_chain_runtime::AccountId, NumberOrHex>,
	>,
	new_validators: Vec<state_chain_runtime::AccountId>,
	current_mab: NumberOrHex,
}

#[rpc(server, client, namespace = "cf_monitoring")]
pub trait MonitoringApi {
	#[method(name = "authorities")]
	fn cf_authorities(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<AuthoritiesInfo>;
	#[method(name = "external_chains_block_height")]
	fn cf_external_chains_block_height(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<ExternalChainsBlockHeight>;
	#[method(name = "btc_utxos")]
	fn cf_btc_utxos(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<BtcUtxos>;
	#[method(name = "dot_aggkey")]
	fn cf_dot_aggkey(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<PolkadotAccountId>;
	#[method(name = "suspended_validators")]
	fn cf_suspended_validators(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<(Offence, u32)>>;
	#[method(name = "epoch_state")]
	fn cf_epoch_state(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<RpcEpochState>;
	#[method(name = "redemptions")]
	fn cf_redemptions(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<RedemptionsInfo>;
	#[method(name = "pending_broadcasts")]
	fn cf_pending_broadcasts_count(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<PendingBroadcasts>;
	#[method(name = "pending_tss_ceremonies")]
	fn cf_pending_tss_ceremonies_count(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<PendingTssCeremonies>;
	#[method(name = "pending_swaps")]
	fn cf_pending_swaps_count(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<u32>;
	#[method(name = "open_deposit_channels")]
	fn cf_open_deposit_channels_count(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<OpenDepositChannels>;
	#[method(name = "fee_imbalance")]
	fn cf_fee_imbalance(&self, at: Option<state_chain_runtime::Hash>)
		-> RpcResult<RpcFeeImbalance>;
	#[method(name = "build_version")]
	fn cf_build_version(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<LastRuntimeUpgradeInfo>;
	#[method(name = "rotation_broadcast_ids")]
	fn cf_rotation_broadcast_ids(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<ActivateKeysBroadcastIds>;
	#[method(name = "sol_nonces")]
	fn cf_sol_nonces(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<SolanaNonces>;
	#[method(name = "sol_aggkey")]
	fn cf_sol_aggkey(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<SolAddress>;
	#[method(name = "sol_onchain_key")]
	fn cf_sol_onchain_key(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<SolAddress>;
	#[method(name = "data")]
	fn cf_monitoring_data(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcMonitoringData>;
	#[method(name = "accounts_info")]
	fn cf_accounts_info(
		&self,
		accounts: BoundedVec<state_chain_runtime::AccountId, ConstU32<10>>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<RpcAccountInfoV2>>;
	#[method(name = "simulate_auction")]
	fn cf_simulate_auction(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<AuctionResult>;
}

impl<C, B, BE> MonitoringApiServer for CustomRpc<C, B, BE>
where
	B: BlockT<Hash = state_chain_runtime::Hash, Header = state_chain_runtime::Header>,
	BE: Send + Sync + 'static,
	C: sp_api::ProvideRuntimeApi<B>
		+ Send
		+ Sync
		+ 'static
		+ HeaderBackend<B>
		+ BlockchainEvents<B>,
	C::Api: MonitoringRuntimeApi<B>,
{
	pass_through! {
		cf_authorities() -> AuthoritiesInfo,
		cf_external_chains_block_height() -> ExternalChainsBlockHeight,
		cf_btc_utxos() -> BtcUtxos,
		cf_dot_aggkey() -> PolkadotAccountId,
		cf_suspended_validators() -> Vec<(Offence, u32)>,
		cf_epoch_state() -> RpcEpochState [map: Into::into],
		cf_redemptions() -> RedemptionsInfo,
		cf_pending_broadcasts_count() -> PendingBroadcasts,
		cf_pending_tss_ceremonies_count() -> PendingTssCeremonies,
		cf_pending_swaps_count() -> u32,
		cf_open_deposit_channels_count() -> OpenDepositChannels,
		cf_build_version() -> LastRuntimeUpgradeInfo,
		cf_rotation_broadcast_ids() -> ActivateKeysBroadcastIds,
		cf_sol_nonces() -> SolanaNonces,
		cf_sol_aggkey() -> SolAddress,
		cf_sol_onchain_key() -> SolAddress,
		cf_monitoring_data() -> RpcMonitoringData [map: Into::into],
	}

	fn cf_fee_imbalance(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcFeeImbalance> {
		self.rpc_backend
			.with_runtime_api::<_, _>(at, |api, hash| api.cf_fee_imbalance(hash))
			.map(|imbalance| imbalance.map(|i| (*i).into()))
	}

	fn cf_accounts_info(
		&self,
		accounts: BoundedVec<state_chain_runtime::AccountId, ConstU32<10>>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<RpcAccountInfoV2>> {
		let accounts_info = self
			.rpc_backend
			.with_runtime_api(at, |api, hash| api.cf_accounts_info(hash, accounts))?;
		Ok(accounts_info
			.into_iter()
			.map(|account_info| RpcAccountInfoV2 {
				balance: account_info.balance.into(),
				bond: account_info.bond.into(),
				last_heartbeat: account_info.last_heartbeat,
				reputation_points: account_info.reputation_points,
				keyholder_epochs: account_info.keyholder_epochs,
				is_current_authority: account_info.is_current_authority,
				#[expect(deprecated)]
				is_current_backup: account_info.is_current_backup,
				is_qualified: account_info.is_qualified,
				is_online: account_info.is_online,
				is_bidding: account_info.is_bidding,
				bound_redeem_address: account_info.bound_redeem_address,
				apy_bp: account_info.apy_bp,
				restricted_balances: account_info.restricted_balances,
				estimated_redeemable_balance: account_info.estimated_redeemable_balance.into(),
			})
			.collect())
	}

	fn cf_simulate_auction(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<AuctionResult> {
		let result = crate::flatten_into_error(
			self.rpc_backend.with_runtime_api(at, |api, hash| api.cf_simulate_auction(hash)),
		)?;
		Ok(AuctionResult {
			auction_outcome: AuctionOutcome {
				winners: result.0.winners,
				bond: result.0.bond.into(),
			},
			operators_info: result
				.1
				.into_iter()
				.map(|(account, snapshot)| (account, snapshot.map_bids(|bid| bid.into())))
				.collect(),
			new_validators: result.2,
			current_mab: result.3.into(),
		})
	}
}
