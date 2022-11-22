use crate::chainflip::Offence;
use cf_chains::eth::SigData;
use codec::{Decode, Encode};
use pallet_cf_governance::GovCallHash;
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_api::decl_runtime_apis;
use sp_core::U256;
use sp_runtime::AccountId32;
use sp_std::vec::Vec;

type VanityName = Vec<u8>;

#[derive(PartialEq, Eq, Clone, Encode, Decode, Copy)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum BackupOrPassive {
	Backup,
	Passive,
}

// TEMP: so frontend doesn't break after removal of passive from backend
#[derive(PartialEq, Eq, Clone, Encode, Decode, Copy)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum ChainflipAccountStateWithPassive {
	CurrentAuthority,
	BackupOrPassive(BackupOrPassive),
}

#[derive(Encode, Decode, Eq, PartialEq)]
pub struct RuntimeApiAccountInfo {
	pub stake: u128,
	pub bond: u128,
	pub last_heartbeat: u32,
	pub is_live: bool,
	pub is_activated: bool,
	pub online_credits: u32,
	pub reputation_points: i32,
	pub withdrawal_address: [u8; 20],
	pub state: ChainflipAccountStateWithPassive,
}

#[derive(Encode, Decode, Eq, PartialEq)]
pub struct RuntimeApiPendingClaim {
	pub amount: U256,
	pub address: [u8; 20],
	pub expiry: U256,
	pub sig_data: SigData,
}

#[derive(Encode, Decode, Eq, PartialEq)]
pub struct RuntimeApiPenalty {
	pub reputation_points: i32,
	pub suspension_duration_blocks: u32,
}

#[derive(Encode, Decode, Eq, PartialEq)]
pub struct AuctionState {
	pub blocks_per_epoch: u32,
	pub current_epoch_started_at: u32,
	pub claim_period_as_percentage: u8,
	pub min_stake: u128,
	pub auction_size_range: (u32, u32),
}

decl_runtime_apis!(
	/// Definition for all runtime API interfaces.
	pub trait CustomRuntimeApi {
		/// Returns true if the current phase is the auction phase.
		fn cf_is_auction_phase() -> bool;
		fn cf_eth_flip_token_address() -> [u8; 20];
		fn cf_eth_stake_manager_address() -> [u8; 20];
		fn cf_eth_key_manager_address() -> [u8; 20];
		fn cf_eth_chain_id() -> u64;
		/// Returns the eth vault in the form [agg_key, active_from_eth_block]
		fn cf_eth_vault() -> ([u8; 33], u32);
		/// Returns the Auction params in the form [min_set_size, max_set_size]
		fn cf_auction_parameters() -> (u32, u32);
		fn cf_min_stake() -> u128;
		fn cf_current_epoch() -> u32;
		fn cf_epoch_duration() -> u32;
		fn cf_current_epoch_started_at() -> u32;
		fn cf_authority_emission_per_block() -> u128;
		fn cf_backup_emission_per_block() -> u128;
		/// Returns the flip supply in the form [total_issuance, offchain_funds]
		fn cf_flip_supply() -> (u128, u128);
		fn cf_accounts() -> Vec<(AccountId32, VanityName)>;
		fn cf_account_info(account_id: AccountId32) -> RuntimeApiAccountInfo;
		fn cf_pending_claim(account_id: AccountId32) -> Option<RuntimeApiPendingClaim>;
		fn cf_get_claim_certificate(account_id: AccountId32) -> Option<Vec<u8>>;
		fn cf_penalties() -> Vec<(Offence, RuntimeApiPenalty)>;
		fn cf_suspensions() -> Vec<(Offence, Vec<(u32, AccountId32)>)>;
		fn cf_generate_gov_key_call_hash(call: Vec<u8>) -> GovCallHash;
		fn cf_auction_state() -> AuctionState;
	}
);
