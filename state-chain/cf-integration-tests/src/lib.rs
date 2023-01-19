#![cfg(test)]

mod network;

mod signer_nomination;

mod mock_runtime;

mod threshold_signing;

mod account;
mod authorities;
mod genesis;
mod governance;
mod new_epoch;
mod staking;
mod swapping;

use frame_support::{assert_noop, assert_ok, traits::OnInitialize};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::crypto::{Pair, Public};
use sp_finality_grandpa::AuthorityId as GrandpaId;
use state_chain_runtime::{
	constants::common::*, opaque::SessionKeys, AccountId, Emissions, Flip, Governance, Reputation,
	Runtime, RuntimeOrigin, Staking, System, Validator, Witnesser,
};

use cf_primitives::{AuthorityCount, BlockNumber, EpochIndex, FlipBalance};
use cf_traits::EpochInfo;
use pallet_cf_staking::{EthTransactionHash, EthereumAddress};
use sp_runtime::AccountId32;

type NodeId = AccountId32;
const ETH_DUMMY_ADDR: EthereumAddress = [42u8; 20];
const ETH_ZERO_ADDRESS: EthereumAddress = [0xff; 20];
const TX_HASH: EthTransactionHash = [211u8; 32];

pub const GENESIS_KEY_SEED: u64 = 42;

// TODO - remove collision of account numbers
pub const ALICE: [u8; 32] = [0xaa; 32];
pub const BOB: [u8; 32] = [0xbb; 32];
pub const CHARLIE: [u8; 32] = [0xcc; 32];
// Root and Gov member
pub const ERIN: [u8; 32] = [0xee; 32];

const GENESIS_EPOCH: EpochIndex = 1;

pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
	TPublic::Pair::from_string(&format!("//{seed}"), None)
		.expect("static values are valid; qed")
		.public()
}

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
