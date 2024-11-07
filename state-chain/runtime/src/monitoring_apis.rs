use crate::{chainflip::Offence, ValidatorInfo};
use cf_chains::{
	dot::PolkadotAccountId,
	sol::{api::DurableNonceAndAccount, SolAddress, SolSignature},
};
use cf_primitives::AssetAmount;
use codec::{Decode, Encode};
use frame_support::sp_runtime::AccountId32;
use pallet_cf_asset_balances::VaultImbalance;
pub use pallet_cf_ingress_egress::OwedAmount;
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
}
#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct BtcUtxos {
	pub total_balance: u64,
	pub count: u32,
}

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct EpochState {
	pub blocks_per_epoch: u32,
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
}
#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct FeeImbalance<A> {
	pub ethereum: VaultImbalance<A>,
	pub polkadot: VaultImbalance<A>,
	pub arbitrum: VaultImbalance<A>,
	pub bitcoin: VaultImbalance<A>,
	pub solana: VaultImbalance<A>,
}

impl<A> FeeImbalance<A> {
	pub fn map<B>(&self, f: impl Fn(&A) -> B) -> FeeImbalance<B> {
		FeeImbalance {
			ethereum: self.ethereum.map(&f),
			polkadot: self.polkadot.map(&f),
			arbitrum: self.arbitrum.map(&f),
			bitcoin: self.bitcoin.map(&f),
			solana: self.solana.map(&f),
		}
	}
}

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct AuthoritiesInfo {
	pub authorities: u32,
	pub online_authorities: u32,
	pub backups: u32,
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

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct MonitoringData {
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
}

impl From<MonitoringData> for MonitoringDataV2 {
	fn from(monitoring_data: MonitoringData) -> Self {
		Self {
			epoch: monitoring_data.epoch,
			pending_redemptions: monitoring_data.pending_redemptions,
			fee_imbalance: monitoring_data.fee_imbalance.map(|i| *i),
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
			flip_supply: monitoring_data.flip_supply,
			sol_aggkey: Default::default(),
			sol_onchain_key: Default::default(),
			sol_nonces: Default::default(),
			activating_key_broadcast_ids: Default::default(),
		}
	}
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
		#[changed_in(2)]
		fn cf_monitoring_data() -> MonitoringData;
		fn cf_monitoring_data() -> MonitoringDataV2;
		fn cf_accounts_info(
			accounts: BoundedVec<AccountId32, sp_core::ConstU32<10>>,
		) -> Vec<ValidatorInfo>;
	}
);
