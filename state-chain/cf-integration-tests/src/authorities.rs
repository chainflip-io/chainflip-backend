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

use crate::{
	genesis, get_validator_state, network, witness_ethereum_rotation_broadcast,
	witness_rotation_broadcasts, AllVaults, ChainflipAccountState, NodeId,
	HEARTBEAT_BLOCK_INTERVAL, VAULT_ROTATION_BLOCKS,
};

use frame_support::{assert_err, assert_ok};
use pallet_cf_vaults::{PendingVaultActivation, VaultActivationStatus};
use sp_runtime::AccountId32;
use std::collections::{BTreeSet, HashMap};

use cf_primitives::{AuthorityCount, FlipBalance, GENESIS_EPOCH};
use cf_traits::{AsyncResult, EpochInfo, KeyRotationStatusOuter, KeyRotator};
use cf_utilities::assert_matches;

use pallet_cf_environment::{PolkadotVaultAccountId, SafeModeUpdate};
use pallet_cf_validator::{CurrentRotationPhase, RotationPhase};
use state_chain_runtime::{
	BitcoinThresholdSigner, Environment, EvmInstance, EvmThresholdSigner, Flip,
	PolkadotCryptoInstance, PolkadotInstance, PolkadotThresholdSigner, Runtime, RuntimeOrigin,
	SolanaInstance, SolanaThresholdSigner, System, Validator,
};

// Helper function that creates a network, funds backup nodes, and have them join the auction.
pub fn fund_authorities_and_join_auction(
	num_backups: AuthorityCount,
) -> (network::Network, Vec<NodeId>, Vec<NodeId>) {
	// Create MAX_AUTHORITIES backup nodes and fund them above our genesis
	// authorities The result will be our newly created nodes will be authorities
	// and the genesis authorities will become backup nodes
	let genesis_authorities: Vec<AccountId32> = Validator::current_authorities();
	let (mut testnet, init_backup_nodes) =
		network::Network::create(num_backups as u8, &genesis_authorities);

	// An initial balance which is greater than the genesis balances
	// We intend for these initially backup nodes to win the auction
	const INITIAL_FUNDING: FlipBalance = genesis::GENESIS_BALANCE * 2;
	// Fund these backup nodes so that they are included in the next epoch
	for node in &init_backup_nodes {
		testnet.state_chain_gateway_contract.fund_account(
			node.clone(),
			INITIAL_FUNDING,
			GENESIS_EPOCH,
		);
	}

	// Allow the funds to be registered, initialise the account keys and peer
	// ids, register as a validator, then start bidding.
	testnet.move_forward_blocks(2);

	for node in &init_backup_nodes {
		network::Cli::register_as_validator(node);
		network::setup_account_and_peer_mapping(node);
		network::Cli::start_bidding(node);
	}

	(testnet, genesis_authorities, init_backup_nodes)
}

/// Tests that Validator and Vaults work together to complete a Authority Rotation
/// by going through the correct sequence in sync.
#[test]
fn authority_rotates_with_correct_sequence() {
	const EPOCH_DURATION_BLOCKS: u32 = 1000;
	const MAX_AUTHORITIES: AuthorityCount = 10;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_DURATION_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			let (mut testnet, _, _) = fund_authorities_and_join_auction(MAX_AUTHORITIES);
			assert_eq!(GENESIS_EPOCH, Validator::epoch_index());

			// Skip the first authority rotation, as key handover is guaranteed to succeed
			// when rotating for the first time.
			testnet.move_to_the_next_epoch();
			witness_ethereum_rotation_broadcast(1);

			assert_matches!(Validator::current_rotation_phase(), RotationPhase::Idle);
			assert_eq!(
				AllVaults::status(),
				AsyncResult::Ready(KeyRotationStatusOuter::RotationComplete)
			);
			assert_eq!(GENESIS_EPOCH + 1, Validator::epoch_index());

			testnet.move_to_the_end_of_epoch();

			// Start the Authority and Vault rotation
			// idle -> Keygen
			testnet.move_forward_blocks(4);
			assert_matches!(
				Validator::current_rotation_phase(),
				RotationPhase::KeygensInProgress(..)
			);
			// NOTE: This happens due to a bug in `move_forward_blocks`: keygen completes in the
			// same block in which is was requested.
			assert_eq!(
				AllVaults::status(),
				AsyncResult::Ready(KeyRotationStatusOuter::KeygenComplete)
			);

			// Key Handover complete.
			testnet.move_forward_blocks(4);
			assert_matches!(
				Validator::current_rotation_phase(),
				RotationPhase::KeyHandoversInProgress(..)
			);
			// NOTE: See above, we skip the pending state.
			assert_eq!(
				AllVaults::status(),
				AsyncResult::Ready(KeyRotationStatusOuter::KeyHandoverComplete)
			);

			// Activate new key.
			// The key is immediately activated in the next block
			testnet.move_forward_blocks(1);
			assert_matches!(Validator::current_rotation_phase(), RotationPhase::ActivatingKeys(..));

			// Wait for an extra block to allow TSS to complete, we switch to RotationComplete once
			// that's done
			testnet.move_forward_blocks(1);
			assert_eq!(
				AllVaults::status(),
				AsyncResult::Ready(KeyRotationStatusOuter::RotationComplete),
				"Rotation should be complete but vault status is {:?}",
				AllVaults::status()
			);

			// Rotating session
			testnet.move_forward_blocks(1);
			assert_matches!(
				Validator::current_rotation_phase(),
				RotationPhase::SessionRotating(..)
			);
			assert_eq!(
				AllVaults::status(),
				AsyncResult::Ready(KeyRotationStatusOuter::RotationComplete)
			);

			// Rotation Completed.
			testnet.move_forward_blocks(1);
			assert_matches!(Validator::current_rotation_phase(), RotationPhase::Idle);
			assert_eq!(
				AllVaults::status(),
				AsyncResult::Ready(KeyRotationStatusOuter::RotationComplete)
			);

			assert_eq!(
				GENESIS_EPOCH + 2,
				Validator::epoch_index(),
				"We should be in the next epoch."
			);
		});
}

#[test]
fn authorities_earn_rewards_for_authoring_blocks() {
	// We want to have at least one heartbeat within our reduced epoch
	const EPOCH_BLOCKS: u32 = 1000;
	// Reduce our validating set and hence the number of nodes we need to have a backup
	// set
	const MAX_AUTHORITIES: AuthorityCount = 3;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			let genesis_authorities = Validator::current_authorities();
			let (mut testnet, _) = network::Network::create(0, &genesis_authorities);

			let funded_amounts = || {
				genesis_authorities
					.iter()
					.map(|id| (id.clone(), Flip::total_balance_of(id)))
					.collect()
			};

			let funded_amounts_before: Vec<(AccountId32, u128)> = funded_amounts();

			// each authority should author a block and mint FLIP to themselves
			testnet.move_forward_blocks(MAX_AUTHORITIES);

			// Each node should have more rewards now than before, since they've each authored a
			// block
			let funded_amounts_after = funded_amounts();

			// Ensure all nodes have increased the same amount
			let first_amount = funded_amounts_after.first().unwrap().1;
			funded_amounts_after.iter().all(|(_node, amount)| amount == &first_amount);

			// Ensure all nodes have a higher balance than before
			funded_amounts_before.into_iter().zip(funded_amounts_after).for_each(
				|((_node, amount_before), (_node2, amount_after))| {
					assert!(amount_before < amount_after)
				},
			);
		});
}

#[test]
fn genesis_nodes_rotated_out_accumulate_rewards_correctly() {
	// We want to have at least one heartbeat within our reduced epoch
	const EPOCH_BLOCKS: u32 = 1000;
	// Reduce our validating set and hence the number of nodes we need to have a backup
	// set
	const MAX_AUTHORITIES: AuthorityCount = 10;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			let (mut testnet, genesis_authorities, init_backup_nodes) =
				fund_authorities_and_join_auction(MAX_AUTHORITIES);

			// Start an auction
			testnet.move_to_the_next_epoch();
			assert_eq!(GENESIS_EPOCH + 1, Validator::epoch_index(), "We should be in a new epoch");

			// assert list of authorities as being the new nodes
			let current_authorities = Validator::current_authorities();

			assert_eq!(
				init_backup_nodes.into_iter().collect::<BTreeSet<AccountId32>>(),
				current_authorities.clone().into_iter().collect::<BTreeSet<AccountId32>>(),
				"our new initial backup nodes should be the new authorities"
			);

			current_authorities.iter().for_each(|account_id| {
				assert_eq!(
					get_validator_state(account_id),
					ChainflipAccountState::CurrentAuthority
				);
				// TODO: Check historical epochs
			});

			// assert list of backup validators as being the genesis authorities
			let highest_funded_backup_nodes =
				Validator::highest_funded_qualified_backup_nodes_lookup();

			assert_eq!(
				genesis_authorities.into_iter().collect::<BTreeSet<AccountId32>>(),
				highest_funded_backup_nodes,
				"the genesis authorities should now be the backup nodes"
			);

			highest_funded_backup_nodes.iter().for_each(|account_id| {
				// we were active in the first epoch
				assert_eq!(get_validator_state(account_id), ChainflipAccountState::Backup);
				// TODO: Check historical epochs
			});

			let backup_node_balances: HashMap<NodeId, FlipBalance> = highest_funded_backup_nodes
				.iter()
				.map(|validator_id| (validator_id.clone(), Flip::total_balance_of(validator_id)))
				.collect::<Vec<(NodeId, FlipBalance)>>()
				.into_iter()
				.collect();

			// Move forward a heartbeat, emissions should be shared to backup nodes
			testnet.move_forward_blocks(HEARTBEAT_BLOCK_INTERVAL);

			// We won't calculate the exact emissions but they should be greater than their
			// initial balance
			for (backup_node, pre_balance) in backup_node_balances {
				assert!(pre_balance < Flip::total_balance_of(&backup_node));
			}
		});
}

#[test]
fn authority_rotation_can_succeed_after_aborted_by_safe_mode() {
	const EPOCH_BLOCKS: u32 = 1000;
	const MAX_AUTHORITIES: AuthorityCount = 10;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			let (mut testnet, _, _) = fund_authorities_and_join_auction(MAX_AUTHORITIES);

			// Resolve Auction
			testnet.move_to_the_end_of_epoch();

			// Run until key gen is completed.
			testnet.move_forward_blocks(4);
			assert!(
				matches!(
					AllVaults::status(),
					AsyncResult::Ready(KeyRotationStatusOuter::KeygenComplete)
				),
				"Keygen should be complete but is {:?}",
				AllVaults::status()
			);

			// This is the last chance to abort validator rotation. Activate code red here.
			assert_ok!(Environment::update_safe_mode(
				pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
				SafeModeUpdate::CodeRed
			));
			testnet.move_forward_blocks(1);

			// Ensure Validator and Vault rotation have been aborted.
			assert_eq!(CurrentRotationPhase::<Runtime>::get(), RotationPhase::Idle);
			assert_eq!(AllVaults::status(), AsyncResult::Void);

			// Authority rotation does not start while in Safe Mode.
			testnet.move_forward_blocks(EPOCH_BLOCKS);

			assert_eq!(CurrentRotationPhase::<Runtime>::get(), RotationPhase::Idle);
			assert_eq!(AllVaults::status(), AsyncResult::Void);

			// Changing to code green should restart the Authority rotation
			assert_ok!(Environment::update_safe_mode(
				pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
				SafeModeUpdate::CodeGreen
			));

			// Authority rotation should be successful.
			testnet.move_forward_blocks(VAULT_ROTATION_BLOCKS);
			assert_eq!(GENESIS_EPOCH + 1, Validator::epoch_index(), "We should be in a new epoch");
		});
}

#[test]
fn authority_rotation_cannot_be_aborted_after_key_handover_and_completes_even_on_safe_mode_enabled()
{
	const EPOCH_BLOCKS: u32 = 1000;
	const MAX_AUTHORITIES: AuthorityCount = 10;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			let (mut testnet, _, _) = fund_authorities_and_join_auction(MAX_AUTHORITIES);

			// Resolve Auction
			testnet.move_to_the_end_of_epoch();

			// Run until key handover starts
			testnet.move_forward_blocks(5);
			assert!(
				matches!(
					AllVaults::status(),
					AsyncResult::Ready(KeyRotationStatusOuter::KeyHandoverComplete)
				),
				"Key handover should be complete but is {:?}",
				AllVaults::status()
			);

			assert_ok!(Environment::update_safe_mode(
				pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
				SafeModeUpdate::CodeRed
			));

			testnet.move_forward_blocks(3);

			// Authority rotation is stalled while in Code Red because of disabling dispatching
			// witness extrinsics and so witnessing vault rotation will be stalled.
			assert_matches!(
				AllVaults::status(),
				AsyncResult::Ready(KeyRotationStatusOuter::RotationComplete)
			);
			testnet.move_forward_blocks(3);
			assert_eq!(GENESIS_EPOCH + 1, Validator::epoch_index(), "We should be in a new epoch");
		});
}

#[test]
fn authority_rotation_can_recover_after_keygen_fails() {
	const EPOCH_BLOCKS: u32 = 1000;
	const MAX_AUTHORITIES: AuthorityCount = 10;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			let (mut testnet, _, backup_nodes) = fund_authorities_and_join_auction(MAX_AUTHORITIES);

			testnet.set_active_all_nodes(false);

			// Begin the rotation, but make Keygen fail.
			testnet.move_to_the_end_of_epoch();

			testnet.move_forward_blocks(1);
			assert_matches!(
				Validator::current_rotation_phase(),
				RotationPhase::KeygensInProgress(..)
			);
			assert_eq!(AllVaults::status(), AsyncResult::Pending);
			backup_nodes.iter().for_each(|validator| {
				assert_ok!(EvmThresholdSigner::report_keygen_outcome(
					RuntimeOrigin::signed(validator.clone()),
					EvmThresholdSigner::ceremony_id_counter(),
					Err(BTreeSet::default()),
				));
				assert_ok!(PolkadotThresholdSigner::report_keygen_outcome(
					RuntimeOrigin::signed(validator.clone()),
					PolkadotThresholdSigner::ceremony_id_counter(),
					Err(BTreeSet::default()),
				));
				assert_ok!(BitcoinThresholdSigner::report_keygen_outcome(
					RuntimeOrigin::signed(validator.clone()),
					BitcoinThresholdSigner::ceremony_id_counter(),
					Err(BTreeSet::default()),
				));
				assert_ok!(SolanaThresholdSigner::report_keygen_outcome(
					RuntimeOrigin::signed(validator.clone()),
					SolanaThresholdSigner::ceremony_id_counter(),
					Err(BTreeSet::default()),
				));
			});

			// Authority rotation can recover and succeed.
			testnet.set_active_all_nodes(true);

			testnet.move_forward_blocks(VAULT_ROTATION_BLOCKS + 1);
			assert_eq!(GENESIS_EPOCH + 1, Validator::epoch_index(), "We should be in a new epoch");
		});
}

#[test]
fn authority_rotation_can_recover_after_key_handover_fails() {
	const EPOCH_BLOCKS: u32 = 1000;
	const MAX_AUTHORITIES: AuthorityCount = 10;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			let (mut testnet, _, backup_nodes) = fund_authorities_and_join_auction(MAX_AUTHORITIES);
			// Rotate authority at least once to ensure epoch keys are set.
			testnet.move_to_the_next_epoch();
			assert_eq!(GENESIS_EPOCH + 1, Validator::epoch_index(), "We should be in a new epoch");

			witness_ethereum_rotation_broadcast(1);

			// Begin the second rotation.
			testnet.move_to_the_end_of_epoch();
			testnet.move_forward_blocks(4);

			// Make Key Handover fail. Only Bitcoin vault can fail during Key Handover.
			// Ethereum and Polkadot do not need to wait for Key Handover.
			testnet.set_active_all_nodes(false);

			testnet.move_forward_blocks(1);
			backup_nodes.iter().for_each(|validator| {
				assert_ok!(BitcoinThresholdSigner::report_key_handover_outcome(
					RuntimeOrigin::signed(validator.clone()),
					BitcoinThresholdSigner::ceremony_id_counter(),
					Err(BTreeSet::default()),
				));
				assert_err!(
					EvmThresholdSigner::report_key_handover_outcome(
						RuntimeOrigin::signed(validator.clone()),
						EvmThresholdSigner::ceremony_id_counter(),
						Err(BTreeSet::default()),
					),
					pallet_cf_threshold_signature::Error::<Runtime, EvmInstance>::InvalidRotationStatus
				);
				assert_err!(
					PolkadotThresholdSigner::report_key_handover_outcome(
						RuntimeOrigin::signed(validator.clone()),
						EvmThresholdSigner::ceremony_id_counter(),
						Err(BTreeSet::default()),
					),
					pallet_cf_threshold_signature::Error::<Runtime, PolkadotCryptoInstance>::InvalidRotationStatus
				);
				assert_err!(
					SolanaThresholdSigner::report_key_handover_outcome(
						RuntimeOrigin::signed(validator.clone()),
						SolanaThresholdSigner::ceremony_id_counter(),
						Err(BTreeSet::default()),
					),
					pallet_cf_threshold_signature::Error::<Runtime, SolanaInstance>::InvalidRotationStatus
				);
			});

			testnet.move_forward_blocks(1);
			assert_matches!(
				Validator::current_rotation_phase(),
				RotationPhase::KeyHandoversInProgress(..)
			);
			assert_eq!(
				AllVaults::status(),
				AsyncResult::Ready(KeyRotationStatusOuter::Failed(BTreeSet::default()))
			);

			// Key handovers are retried after failure.
			// Authority rotation can recover and succeed.
			testnet.set_active_all_nodes(true);

			testnet.move_forward_blocks(VAULT_ROTATION_BLOCKS);
			assert_eq!(GENESIS_EPOCH + 2, Validator::epoch_index(), "We should be in a new epoch");
		});
}

/// Tests that Validator and Vaults work together to complete a Authority Rotation
/// by going through the correct sequence in sync.
#[test]
fn can_move_through_multiple_epochs() {
	const EPOCH_BLOCKS: u32 = 100;
	const MAX_AUTHORITIES: AuthorityCount = 10;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			let (mut testnet, _, _) = fund_authorities_and_join_auction(MAX_AUTHORITIES);
			assert_eq!(GENESIS_EPOCH, Validator::epoch_index());
			testnet.move_to_the_next_epoch();
			witness_ethereum_rotation_broadcast(1);

			for i in 1..21 {
				testnet.move_to_the_next_epoch();
				witness_rotation_broadcasts([i + 1, i, i, i, i, i]);
			}
			assert_eq!(GENESIS_EPOCH + 21, Validator::epoch_index());
		});
}

#[test]
fn cant_rotate_if_previous_rotation_is_pending() {
	const EPOCH_BLOCKS: u32 = 100;
	const MAX_AUTHORITIES: AuthorityCount = 10;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			let (mut testnet, _, _) = fund_authorities_and_join_auction(MAX_AUTHORITIES);
			assert_eq!(GENESIS_EPOCH, Validator::epoch_index());
			testnet.move_to_the_next_epoch();
			witness_ethereum_rotation_broadcast(1);

			testnet.move_to_the_next_epoch();
			let epoch_index = Validator::epoch_index();

			// we try to rotate again but cant since the rotation broadcast txs of the last epoch
			// are still pending.
			testnet.move_to_the_end_of_epoch();
			testnet.move_forward_blocks(VAULT_ROTATION_BLOCKS);
			System::assert_last_event(state_chain_runtime::RuntimeEvent::Validator(
				pallet_cf_validator::Event::<Runtime>::PreviousRotationStillPending,
			));
			// we are still in the older epoch
			assert_eq!(epoch_index, Validator::epoch_index());

			// we witness the rotation txs of the older epoch which then causes the rotation to
			// start on the next block.
			witness_rotation_broadcasts([2, 1, 1, 1, 1, 1]);
			testnet.move_forward_blocks(VAULT_ROTATION_BLOCKS);
			assert_eq!(epoch_index + 1, Validator::epoch_index());
		});
}

#[test]
fn waits_for_governance_when_apicall_fails() {
	const EPOCH_BLOCKS: u32 = 100;
	const MAX_AUTHORITIES: AuthorityCount = 10;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			let (mut testnet, _, _) = fund_authorities_and_join_auction(MAX_AUTHORITIES);
			assert_eq!(GENESIS_EPOCH, Validator::epoch_index());
			testnet.move_to_the_next_epoch();
			witness_ethereum_rotation_broadcast(1);

			let epoch_index = Validator::epoch_index();

			PolkadotVaultAccountId::<Runtime>::set(None);
			testnet.move_to_the_end_of_epoch();
			testnet.move_forward_blocks(VAULT_ROTATION_BLOCKS);

			// we are still in old epoch
			assert_eq!(Validator::epoch_index(), epoch_index);
			// rotation has not completed
			assert_matches!(
				CurrentRotationPhase::<Runtime>::get(),
				RotationPhase::ActivatingKeys(..)
			);

			assert_matches!(
				PendingVaultActivation::<Runtime, PolkadotInstance>::get().unwrap(),
				VaultActivationStatus::ActivationFailedAwaitingGovernance { .. }
			);

			// move forward a few blocks
			testnet.move_forward_blocks(10);

			assert_matches!(
				PendingVaultActivation::<Runtime, PolkadotInstance>::get().unwrap(),
				VaultActivationStatus::ActivationFailedAwaitingGovernance { .. }
			);

			// we are still in old epoch
			assert_eq!(Validator::epoch_index(), epoch_index);
			// rotation is still stalled
			assert_matches!(
				CurrentRotationPhase::<Runtime>::get(),
				RotationPhase::ActivatingKeys(..)
			);

			// manually setting the activation status to complete completes the rotation.
			PendingVaultActivation::<Runtime, PolkadotInstance>::put(
				VaultActivationStatus::Complete,
			);
			testnet.move_forward_blocks(5);

			// we have moved to the new epoch
			assert_eq!(Validator::epoch_index(), epoch_index + 1);
			assert_matches!(CurrentRotationPhase::<Runtime>::get(), RotationPhase::Idle);
		});
}
