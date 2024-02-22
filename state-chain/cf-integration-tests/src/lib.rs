#![cfg(test)]
#![feature(exclusive_range_pattern)]
mod network;

mod broadcasting;
mod mock_runtime;
mod signer_nomination;
mod threshold_signing;

mod account;
mod authorities;
mod funding;
mod genesis;
mod governance;
mod new_epoch;
mod swapping;
mod witnessing;

use cf_chains::eth::Address as EthereumAddress;
use cf_primitives::{AuthorityCount, BlockNumber, FlipBalance};
use cf_traits::EpochInfo;
use frame_support::{assert_noop, assert_ok, sp_runtime::AccountId32, traits::OnInitialize};
use pallet_cf_funding::EthTransactionHash;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_core::crypto::Pair;
use state_chain_runtime::{
	constants::common::*, opaque::SessionKeys, AccountId, BitcoinVault, Emissions, EthereumVault,
	Flip, Funding, Governance, PolkadotVault, Reputation, Runtime, RuntimeCall, RuntimeOrigin,
	System, Validator, Witnesser,
};

type NodeId = AccountId32;
const ETH_DUMMY_ADDR: EthereumAddress = EthereumAddress::repeat_byte(42u8);
const ETH_ZERO_ADDRESS: EthereumAddress = EthereumAddress::repeat_byte(0xff);
const TX_HASH: EthTransactionHash = [211u8; 32];

pub const GENESIS_KEY_SEED: u64 = 42;

// Validators
pub const ALICE: [u8; 32] = [0xf0; 32];
pub const BOB: [u8; 32] = [0xf1; 32];
pub const CHARLIE: [u8; 32] = [0xf2; 32];
// Root and Gov member
pub const ERIN: [u8; 32] = [0xf3; 32];
// Broker
pub const BROKER: [u8; 32] = [0xf4; 32];
// Liquidity Provider
pub const LIQUIDITY_PROVIDER: [u8; 32] = [0xf5; 32];

pub fn get_validator_state(account_id: &AccountId) -> ChainflipAccountState {
	if Validator::current_authorities().contains(account_id) {
		ChainflipAccountState::CurrentAuthority
	} else {
		ChainflipAccountState::Backup
	}
}

// The minimum number of blocks a vault rotation should last
// 4 (keygen + key verification) + 4(key handover) + 2(activating_key) + 2(session rotating)
const VAULT_ROTATION_BLOCKS: BlockNumber = 12;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum ChainflipAccountState {
	CurrentAuthority,
	Backup,
}

pub type AllVaults = <Runtime as pallet_cf_validator::Config>::KeyRotator;

/// Helper function that dispatches a call that requires EnsureWitnessed origin.
pub fn witness_call(call: RuntimeCall) {
	let epoch = Validator::epoch_index();
	let boxed_call = Box::new(call);
	for node in Validator::current_authorities() {
		assert_ok!(Witnesser::witness_at_epoch(
			RuntimeOrigin::signed(node),
			boxed_call.clone(),
			epoch,
		));
	}
}
