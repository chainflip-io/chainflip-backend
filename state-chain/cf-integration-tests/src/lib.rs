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

#![cfg(test)]
mod network;

mod broadcasting;
mod mock_runtime;
mod signer_nomination;
mod threshold_signing;

mod account;
mod authorities;
mod delegation;
mod fee_scaling;
mod funding;
mod genesis;
mod governance;
mod lending;
mod new_epoch;
mod solana;
mod swapping;
mod trading_strategy;
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
	SolanaVault, System, Validator, Witnesser,
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

pub fn is_current_authority(account_id: &AccountId) -> bool {
	Validator::current_authorities().contains(account_id)
}

// The minimum number of blocks a vault rotation should last
// 4 (keygen + key verification) + 4(key handover) + 2(activating_key) + 2(session rotating)
const VAULT_ROTATION_BLOCKS: BlockNumber = 12;

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
