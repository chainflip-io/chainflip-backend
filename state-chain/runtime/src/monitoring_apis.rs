use crate::{chainflip::Offence, ValidatorInfo};
use cf_chains::dot::PolkadotAccountId;
use codec::{Decode, Encode};
use frame_support::sp_runtime::AccountId32;
pub use pallet_cf_ingress_egress::OwedAmount;
use scale_info::{prelude::string::String, TypeInfo};
use serde::{Deserialize, Serialize};
use sp_api::decl_runtime_apis;
use sp_runtime::BoundedVec;
use sp_std::vec::Vec;

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug)]
pub struct ExternalChainsBlockHeight {
	pub bitcoin: u64,
	pub ethereum: u64,
	pub polkadot: u64,
}
#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug)]
pub struct BtcUtxos {
	pub total_balance: u64,
	pub count: u32,
}

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug)]
pub struct EpochState {
	pub blocks_per_epoch: u32,
	pub current_epoch_started_at: u32,
	pub current_epoch_index: u32,
	pub min_active_bid: Option<u128>,
	pub rotation_phase: String,
}

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug)]
pub struct RedemptionsInfo {
	pub total_balance: u128,
	pub count: u32,
}
#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug)]
pub struct PendingBroadcasts {
	pub ethereum: u32,
	pub bitcoin: u32,
	pub polkadot: u32,
	pub arbitrum: u32,
}
#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug)]
pub struct PendingTssCeremonies {
	pub evm: u32,
	pub bitcoin: u32,
	pub polkadot: u32,
}
#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug)]
pub struct OpenDepositChannels {
	pub ethereum: u32,
	pub bitcoin: u32,
	pub polkadot: u32,
	pub arbitrum: u32,
}
#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug)]
pub struct FeeImbalance {
	pub ethereum: u128,
	pub bitcoin: u128,
	pub polkadot: u128,
	pub arbitrum: u128,
}
#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug)]
pub struct AuthoritiesInfo {
	pub authorities: u32,
	pub online_authorities: u32,
	pub backups: u32,
	pub online_backups: u32,
}

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug)]
pub struct LastRuntimeUpgradeInfo {
	pub spec_version: u32,
	pub spec_name: sp_runtime::RuntimeString,
}

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug)]
pub struct FlipSupply {
	pub total_supply: u128,
	pub offchain_supply: u128,
}

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug)]
pub struct MonitoringData {
	pub external_chains_height: ExternalChainsBlockHeight,
	pub btc_utxos: BtcUtxos,
	pub epoch: EpochState,
	pub pending_redemptions: RedemptionsInfo,
	pub pending_broadcasts: PendingBroadcasts,
	pub pending_tss: PendingTssCeremonies,
	pub open_deposit_channels: OpenDepositChannels,
	pub fee_imbalance: FeeImbalance,
	pub authorities: AuthoritiesInfo,
	pub build_version: LastRuntimeUpgradeInfo,
	pub suspended_validators: Vec<(Offence, u32)>,
	pub pending_swaps: u32,
	pub dot_aggkey: PolkadotAccountId,
	pub flip_supply: FlipSupply,
}

decl_runtime_apis!(
	pub trait MonitoringRuntimeApi {
		fn cf_authorities() -> AuthoritiesInfo;
		fn cf_external_chains_block_height() -> ExternalChainsBlockHeight;
		fn cf_btc_utxos() -> BtcUtxos;
		fn cf_dot_aggkey() -> PolkadotAccountId;
		fn cf_suspended_validators() -> Vec<(Offence, u32)>;
		fn cf_epoch_state() -> EpochState;
		fn cf_redemptions() -> RedemptionsInfo;
		fn cf_pending_broadcasts() -> PendingBroadcasts;
		fn cf_pending_tss_ceremonies() -> PendingTssCeremonies;
		fn cf_pending_swaps() -> u32;
		fn cf_open_deposit_channels() -> OpenDepositChannels;
		fn cf_fee_imbalance() -> FeeImbalance;
		fn cf_build_version() -> LastRuntimeUpgradeInfo;
		fn cf_monitoring_data() -> MonitoringData;
		fn cf_accounts_info(
			accounts: BoundedVec<AccountId32, sp_core::ConstU32<10>>,
		) -> Vec<ValidatorInfo>;
	}
);
