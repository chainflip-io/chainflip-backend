#![cfg(test)]
#![feature(exclusive_range_pattern)]
mod network;

mod signer_nomination;

mod mock_runtime;

mod threshold_signing;

mod account;
mod authorities;
mod funding;
mod genesis;
mod governance;
mod new_epoch;
mod swapping;

use cf_chains::eth::Address as EthereumAddress;
use cf_primitives::{AuthorityCount, BlockNumber, FlipBalance};
use cf_traits::EpochInfo;
use frame_support::{assert_noop, assert_ok, traits::OnInitialize};
use pallet_cf_funding::EthTransactionHash;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::crypto::Pair;
use sp_finality_grandpa::AuthorityId as GrandpaId;
use sp_runtime::AccountId32;
use state_chain_runtime::{
	constants::common::*, opaque::SessionKeys, AccountId, Emissions, Flip, Funding, Governance,
	Reputation, Runtime, RuntimeOrigin, System, Validator,
};

type NodeId = AccountId32;
const ETH_DUMMY_ADDR: EthereumAddress = EthereumAddress::repeat_byte(42u8);
const ETH_ZERO_ADDRESS: EthereumAddress = EthereumAddress::repeat_byte(0xff);
const TX_HASH: EthTransactionHash = [211u8; 32];

pub const GENESIS_KEY_SEED: u64 = 42;

// TODO - remove collision of account numbers
pub const ALICE: [u8; 32] = [0xaa; 32];
pub const BOB: [u8; 32] = [0xbb; 32];
pub const CHARLIE: [u8; 32] = [0xcc; 32];
// Root and Gov member
pub const ERIN: [u8; 32] = [0xee; 32];

pub fn get_validator_state(account_id: &AccountId) -> ChainflipAccountState {
	if Validator::current_authorities().contains(account_id) {
		ChainflipAccountState::CurrentAuthority
	} else {
		ChainflipAccountState::Backup
	}
}

// The minimum number of blocks a vault rotation should last
const VAULT_ROTATION_BLOCKS: BlockNumber = 6;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum ChainflipAccountState {
	CurrentAuthority,
	Backup,
}
