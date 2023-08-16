use crate::chainflip::Offence;
use cf_amm::common::Price;
use cf_chains::{
	btc::BitcoinNetwork,
	dot::PolkadotHash,
	eth::{api::EthereumChainId, Address as EthereumAddress},
};
use cf_primitives::{Asset, AssetAmount, EpochIndex, SemVer, SwapOutput};
use codec::{Decode, Encode};
use frame_support::sp_runtime::AccountId32;
use pallet_cf_governance::GovCallHash;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_api::decl_runtime_apis;
use sp_std::vec::Vec;

type VanityName = Vec<u8>;

#[derive(PartialEq, Eq, Clone, Encode, Decode, Copy, TypeInfo, Serialize, Deserialize)]
pub enum BackupOrPassive {
	Backup,
	Passive,
}

// TEMP: so frontend doesn't break after removal of passive from backend
#[derive(PartialEq, Eq, Clone, Encode, Decode, Copy, TypeInfo, Serialize, Deserialize)]
pub enum ChainflipAccountStateWithPassive {
	CurrentAuthority,
	BackupOrPassive(BackupOrPassive),
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo)]
pub struct RuntimeApiAccountInfo {
	pub balance: u128,
	pub bond: u128,
	pub last_heartbeat: u32,
	pub is_live: bool,
	pub is_activated: bool,
	pub online_credits: u32,
	pub reputation_points: i32,
	pub state: ChainflipAccountStateWithPassive,
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo)]
pub struct RuntimeApiAccountInfoV2 {
	pub balance: u128,
	pub bond: u128,
	pub last_heartbeat: u32, // can *maybe* remove this - check with Andrew
	pub online_credits: u32,
	pub reputation_points: i32,
	pub keyholder_epochs: Vec<EpochIndex>,
	pub is_current_authority: bool,
	pub is_current_backup: bool,
	pub is_qualified: bool,
	pub is_online: bool,
	pub is_bidding: bool,
	pub bound_redeem_address: Option<EthereumAddress>,
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo)]
pub struct RuntimeApiPenalty {
	pub reputation_points: i32,
	pub suspension_duration_blocks: u32,
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo)]
pub struct AuctionState {
	pub blocks_per_epoch: u32,
	pub current_epoch_started_at: u32,
	pub redemption_period_as_percentage: u8,
	pub min_funding: u128,
	pub auction_size_range: (u32, u32),
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo)]
pub struct Environment {
	pub bitcoin_network: BitcoinNetwork,
	pub ethereum_chain_id: EthereumChainId,
	pub polkadot_genesis_hash: PolkadotHash,
}

decl_runtime_apis!(
	/// Definition for all runtime API interfaces.
	pub trait CustomRuntimeApi {
		/// Returns true if the current phase is the auction phase.
		fn cf_is_auction_phase() -> bool;
		fn cf_eth_flip_token_address() -> EthereumAddress;
		fn cf_eth_state_chain_gateway_address() -> EthereumAddress;
		fn cf_eth_key_manager_address() -> EthereumAddress;
		fn cf_eth_chain_id() -> u64;
		/// Returns the eth vault in the form [agg_key, active_from_eth_block]
		fn cf_eth_vault() -> ([u8; 33], u32);
		/// Returns the Auction params in the form [min_set_size, max_set_size]
		fn cf_auction_parameters() -> (u32, u32);
		fn cf_min_funding() -> u128;
		fn cf_current_epoch() -> u32;
		fn cf_current_compatibility_version() -> SemVer;
		fn cf_epoch_duration() -> u32;
		fn cf_current_epoch_started_at() -> u32;
		fn cf_authority_emission_per_block() -> u128;
		fn cf_backup_emission_per_block() -> u128;
		/// Returns the flip supply in the form [total_issuance, offchain_funds]
		fn cf_flip_supply() -> (u128, u128);
		fn cf_accounts() -> Vec<(AccountId32, VanityName)>;
		fn cf_account_info(account_id: AccountId32) -> RuntimeApiAccountInfo;
		fn cf_account_info_v2(account_id: AccountId32) -> RuntimeApiAccountInfoV2;
		fn cf_penalties() -> Vec<(Offence, RuntimeApiPenalty)>;
		fn cf_suspensions() -> Vec<(Offence, Vec<(u32, AccountId32)>)>;
		fn cf_generate_gov_key_call_hash(call: Vec<u8>) -> GovCallHash;
		fn cf_auction_state() -> AuctionState;
		fn cf_pool_price(from: Asset, to: Asset) -> Option<Price>;
		fn cf_pool_simulate_swap(from: Asset, to: Asset, amount: AssetAmount)
			-> Option<SwapOutput>;
		fn cf_environment() -> Environment;
		fn cf_min_swap_amount(asset: Asset) -> AssetAmount;
	}
);
