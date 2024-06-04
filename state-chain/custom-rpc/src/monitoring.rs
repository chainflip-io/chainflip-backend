use crate::{to_rpc_error, BlockT, CustomRpc, RpcAccountInfoV2, RpcMonitoringData};
use cf_chains::dot::PolkadotAccountId;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sc_client_api::{BlockchainEvents, HeaderBackend};
use sp_core::{bounded_vec::BoundedVec, ConstU32};
use state_chain_runtime::{
	chainflip::Offence,
	monitoring_apis::{
		AuthoritiesInfo, BtcUtxos, EpochState, ExternalChainsBlockHeight, FeeImbalance,
		LastRuntimeUpgradeInfo, MonitoringRuntimeApi, OpenDepositChannels, PendingBroadcasts,
		PendingTssCeremonies, RedemptionsInfo,
	},
};

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
	fn cf_epoch_state(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<EpochState>;
	#[method(name = "redemptions")]
	fn cf_redemptions(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<RedemptionsInfo>;
	#[method(name = "pending_broadcasts")]
	fn cf_pending_broadcasts(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<PendingBroadcasts>;
	#[method(name = "pending_tss_ceremonies")]
	fn cf_pending_tss_ceremonies(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<PendingTssCeremonies>;
	#[method(name = "pending_swaps")]
	fn cf_pending_swaps(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<u32>;
	#[method(name = "open_deposit_channels")]
	fn cf_open_deposit_channels(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<OpenDepositChannels>;
	#[method(name = "fee_imbalance")]
	fn cf_fee_imbalance(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<FeeImbalance>;
	#[method(name = "build_version")]
	fn cf_build_version(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<LastRuntimeUpgradeInfo>;
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

macro_rules! pass_through {
	($( $name:ident -> $result_type:ty ),+) => {

		$(
			fn $name(&self, at: Option<state_chain_runtime::Hash>) -> RpcResult<$result_type> {
				self.client
					.runtime_api()
					.$name(self.unwrap_or_best(at))
					.map_err(to_rpc_error)
			}
		)+
	};
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
		cf_authorities -> AuthoritiesInfo,
		cf_external_chains_block_height -> ExternalChainsBlockHeight,
		cf_btc_utxos -> BtcUtxos,
		cf_dot_aggkey -> PolkadotAccountId,
		cf_suspended_validators -> Vec<(Offence, u32)>,
		cf_epoch_state -> EpochState,
		cf_redemptions -> RedemptionsInfo,
		cf_pending_broadcasts -> PendingBroadcasts,
		cf_pending_tss_ceremonies -> PendingTssCeremonies,
		cf_pending_swaps -> u32,
		cf_open_deposit_channels -> OpenDepositChannels,
		cf_fee_imbalance -> FeeImbalance,
		cf_build_version -> LastRuntimeUpgradeInfo
	}
	fn cf_monitoring_data(
		&self,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<RpcMonitoringData> {
		self.client
			.runtime_api()
			.cf_monitoring_data(self.unwrap_or_best(at))
			.map(Into::into)
			.map_err(to_rpc_error)
	}
	fn cf_accounts_info(
		&self,
		accounts: BoundedVec<state_chain_runtime::AccountId, ConstU32<10>>,
		at: Option<state_chain_runtime::Hash>,
	) -> RpcResult<Vec<RpcAccountInfoV2>> {
		let accounts_info = self
			.client
			.runtime_api()
			.cf_accounts_info(self.unwrap_or_best(at), accounts)
			.map_err(to_rpc_error)?;
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
