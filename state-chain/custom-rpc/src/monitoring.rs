use super::pass_through;
use crate::{BlockT, CustomRpc, RpcAccountInfoV2, RpcFeeImbalance, RpcMonitoringData, RpcResult};
use cf_chains::{dot::PolkadotAccountId, sol::SolAddress};
use cf_utilities::rpc::NumberOrHex;
use jsonrpsee::proc_macros::rpc;
use sc_client_api::{BlockchainEvents, HeaderBackend};
use serde::{Deserialize, Serialize};
use sp_core::{bounded_vec::BoundedVec, ConstU32};
use state_chain_runtime::{
	chainflip::Offence,
	monitoring_apis::{
		ActivateKeysBroadcastIds, AuthoritiesInfo, BtcUtxos, EpochState, ExternalChainsBlockHeight,
		LastRuntimeUpgradeInfo, MonitoringRuntimeApi, OpenDepositChannels, PendingBroadcasts,
		PendingTssCeremonies, RedemptionsInfo, SolanaNonces,
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
pub struct RpcEpochState {
	pub epoch_duration: u32,
	pub current_epoch_started_at: u32,
	pub current_epoch_index: u32,
	pub min_active_bid: Option<NumberOrHex>,
	pub rotation_phase: String,
}

// Temporary struct to hold the deprecated blocks_per_epoch field.
// Can be deleted after v1.7 is released (meaning: after the version is bumped to 1.8).
#[derive(Serialize, Deserialize, Clone)]
pub struct RpcEpochStateV2 {
	#[deprecated(
		since = "1.8.0",
		note = "This field is deprecated and will be removed in v1.8. Use blocks_per_epoch instead."
	)]
	blocks_per_epoch: u32,
	#[serde(flatten)]
	epoch_state: RpcEpochState,
}

impl From<EpochState> for RpcEpochStateV2 {
	fn from(epoch_state: EpochState) -> Self {
		Self { blocks_per_epoch: epoch_state.epoch_duration, epoch_state: epoch_state.into() }
	}
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
	fn cf_epoch_state(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<RpcEpochStateV2>;
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
}

impl<C, B> MonitoringApiServer for CustomRpc<C, B>
where
	B: BlockT<Hash = state_chain_runtime::Hash, Header = state_chain_runtime::Header>,
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
		cf_epoch_state() -> RpcEpochStateV2 [map: RpcEpochStateV2::from],
		cf_redemptions() -> RedemptionsInfo,
		cf_pending_broadcasts_count() -> PendingBroadcasts,
		cf_pending_tss_ceremonies_count() -> PendingTssCeremonies,
		cf_pending_swaps_count() -> u32,
		cf_open_deposit_channels_count() -> OpenDepositChannels,
		cf_build_version() -> LastRuntimeUpgradeInfo,
		cf_rotation_broadcast_ids() -> ActivateKeysBroadcastIds,
		cf_sol_nonces() -> SolanaNonces,
		cf_sol_aggkey() -> SolAddress,
		cf_sol_onchain_key() -> SolAddress
	}

	fn cf_fee_imbalance(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcFeeImbalance> {
		self.with_runtime_api::<_, _>(at, |api, hash| api.cf_fee_imbalance(hash))
			.map(|imbalance| imbalance.map(|i| (*i).into()))
	}

	fn cf_monitoring_data(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcMonitoringData> {
		self.with_runtime_api(at, |api, hash| api.cf_monitoring_data(hash))
			.map(Into::into)
	}
	fn cf_accounts_info(
		&self,
		accounts: BoundedVec<state_chain_runtime::AccountId, ConstU32<10>>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<RpcAccountInfoV2>> {
		let accounts_info =
			self.with_runtime_api(at, |api, hash| api.cf_accounts_info(hash, accounts))?;
		Ok(accounts_info
			.into_iter()
			.map(|account_info| RpcAccountInfoV2 {
				balance: account_info.balance.into(),
				bond: account_info.bond.into(),
				last_heartbeat: account_info.last_heartbeat,
				reputation_points: account_info.reputation_points,
				keyholder_epochs: account_info.keyholder_epochs,
				is_current_authority: account_info.is_current_authority,
				is_current_backup: account_info.is_current_backup,
				is_qualified: account_info.is_qualified,
				is_online: account_info.is_online,
				is_bidding: account_info.is_bidding,
				bound_redeem_address: account_info.bound_redeem_address,
				apy_bp: account_info.apy_bp,
				restricted_balances: account_info.restricted_balances,
			})
			.collect())
	}
}
