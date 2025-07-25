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

use core::ops::Range;

use crate::{mock::*, Error, *};
use cf_test_utilities::{assert_event_sequence, last_event};
use cf_traits::{
	mocks::{
		bonding::MockBonderFor,
		cfe_interface_mock::{MockCfeEvent, MockCfeInterface},
		key_rotator::MockKeyRotatorA,
		reputation_resetter::MockReputationResetter,
	},
	AccountRoleRegistry, SafeMode, SetSafeMode,
};
use cf_utilities::{assert_matches, success_threshold_from_share_count};
use frame_support::{
	assert_noop, assert_ok,
	error::BadOrigin,
	traits::{HandleLifetime, OriginTrait},
};
use frame_system::RawOrigin;
use sp_runtime::testing::UintAuthorityId;

const ALICE: u64 = 100;
const BOB: u64 = 101;
const GENESIS_EPOCH: u32 = 1;

const OPERATOR_SETTINGS: OperatorSettings =
	OperatorSettings { fee_bps: 250, delegation_acceptance: DelegationAcceptance::Allow };

fn assert_epoch_index(n: EpochIndex) {
	assert_eq!(
		ValidatorPallet::epoch_index(),
		n,
		"we should be in epoch {n:?}. KeyRotator says {:?} / {:?}",
		CurrentRotationPhase::<Test>::get(),
		<Test as crate::Config>::KeyRotator::status()
	);
}

macro_rules! assert_rotation_phase_matches {
	($expected_phase: pat) => {
		assert!(
			matches!(CurrentRotationPhase::<Test>::get(), $expected_phase),
			"Expected {}, got {:?}",
			stringify!($expected_phase),
			CurrentRotationPhase::<Test>::get(),
		);
	};
}

macro_rules! assert_default_rotation_outcome {
	() => {
		assert_rotation_phase_matches!(RotationPhase::Idle);
		assert_epoch_index(GENESIS_EPOCH + 1);
		assert_eq!(Bond::<Test>::get(), EXPECTED_BOND, "bond should be updated");
		// Use BTreeSet to ignore ordering.
		assert_eq!(
			ValidatorPallet::current_authorities().into_iter().collect::<BTreeSet<u64>>(),
			WINNING_BIDS.into_iter().map(|bid| bid.bidder_id).collect::<BTreeSet<_>>()
		);
	};
}

#[track_caller]
fn assert_rotation_aborted() {
	assert_rotation_phase_matches!(RotationPhase::Idle);
	assert_eq!(<Test as Config>::KeyRotator::status(), AsyncResult::Void);
	assert_event_sequence!(
		Test,
		RuntimeEvent::ValidatorPallet(Event::RotationPhaseUpdated {
			new_phase: RotationPhase::Idle
		}),
		RuntimeEvent::ValidatorPallet(Event::RotationAborted)
	);
}

fn add_bids(bids: Vec<Bid<ValidatorId, Amount>>) {
	bids.into_iter().for_each(|bid| {
		MockFlip::credit_funds(&bid.bidder_id, bid.amount);
		// Some account might have already registered, so it's Ok if this fails.
		let _ = <<Test as Chainflip>::AccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(&bid.bidder_id);
		assert_ok!(ValidatorPallet::start_bidding(RuntimeOrigin::signed(bid.bidder_id)));

	})
}

fn remove_bids(bidders: Vec<ValidatorId>) {
	bidders.into_iter().for_each(|bidder| {
		assert_ok!(ValidatorPallet::stop_bidding(RuntimeOrigin::signed(bidder)));
	})
}

fn set_default_test_bids() {
	add_bids([&WINNING_BIDS[..], &LOSING_BIDS[..], &[UNQUALIFIED_BID]].concat());
}

#[test]
fn changing_epoch_block_size() {
	new_test_ext().then_execute_with_checks(|| {
		assert_noop!(
			ValidatorPallet::update_pallet_config(
				RuntimeOrigin::root(),
				PalletConfigUpdate::EpochDuration { blocks: 0 }
			),
			Error::<Test>::InvalidEpochDuration
		);
		const UPDATE: PalletConfigUpdate = PalletConfigUpdate::EpochDuration { blocks: 100 };
		assert_ok!(ValidatorPallet::update_pallet_config(RuntimeOrigin::root(), UPDATE));
		assert_eq!(
			last_event::<Test>(),
			mock::RuntimeEvent::ValidatorPallet(Event::PalletConfigUpdated { update: UPDATE }),
		);
	});
}

#[test]
fn should_retry_rotation_until_success_with_failing_auctions() {
	new_test_ext()
		.execute_with(|| {
			// Stop all current bidders
			ValidatorPallet::get_active_bids().into_iter().for_each(|v| {
				assert_ok!(ValidatorPallet::stop_bidding(RuntimeOrigin::signed(v.bidder_id)));
			});
			assert_eq!(ValidatorPallet::get_active_bids().len(), 0);
		})
		// Move forward past the epoch boundary, the auction will be failing
		.then_advance_n_blocks_and_execute_with_checks(EPOCH_DURATION + 100, || {
			assert_epoch_index(GENESIS_EPOCH);
			assert_eq!(CurrentRotationPhase::<Test>::get(), RotationPhase::<Test>::Idle);

			set_default_test_bids();
		})
		// Now that we have bidders, we should succeed the auction, and complete the rotation
		.then_advance_n_blocks_and_execute_with_checks(1, || {
			assert_matches!(
				CurrentRotationPhase::<Test>::get(),
				RotationPhase::<Test>::KeygensInProgress(..)
			);
			MockKeyRotatorA::keygen_success();
		})
		.then_advance_n_blocks_and_execute_with_checks(2, || {
			assert_matches!(
				CurrentRotationPhase::<Test>::get(),
				RotationPhase::<Test>::KeyHandoversInProgress(..)
			);
			MockKeyRotatorA::key_handover_success();
		})
		.then_advance_n_blocks_and_execute_with_checks(2, || {
			assert_matches!(
				CurrentRotationPhase::<Test>::get(),
				RotationPhase::<Test>::ActivatingKeys(..)
			);
			MockKeyRotatorA::keys_activated();
		})
		.then_advance_n_blocks_and_execute_with_checks(2, || {
			assert_default_rotation_outcome!();
		});
}

#[test]
fn should_be_unable_to_force_rotation_during_a_rotation() {
	new_test_ext().then_execute_with_checks(|| {
		set_default_test_bids();
		ValidatorPallet::start_authority_rotation();
		assert_rotation_phase_matches!(RotationPhase::KeygensInProgress(..));
		assert_noop!(
			ValidatorPallet::force_rotation(RuntimeOrigin::root()),
			Error::<Test>::RotationInProgress
		);
	});
}

#[test]
fn should_rotate_when_forced() {
	new_test_ext().then_execute_with_checks(|| {
		set_default_test_bids();
		assert_noop!(
			ValidatorPallet::force_rotation(RuntimeOrigin::signed(ALICE)),
			sp_runtime::traits::BadOrigin
		);
		assert_ok!(ValidatorPallet::force_rotation(RuntimeOrigin::root()));
		assert_rotation_phase_matches!(RotationPhase::KeygensInProgress(..));
		assert_noop!(
			ValidatorPallet::force_rotation(RuntimeOrigin::root()),
			Error::<Test>::RotationInProgress
		);
	});
}

#[test]
fn auction_winners_should_be_the_new_authorities_on_new_epoch() {
	let genesis_set = BTreeSet::from(GENESIS_AUTHORITIES);
	new_test_ext()
		.then_execute_with_checks(|| {
			assert_eq!(
				CurrentAuthorities::<Test>::get().into_iter().collect::<BTreeSet<u64>>(),
				genesis_set,
				"the current authorities should be the genesis authorities"
			);
			// Run to the epoch boundary.
			set_default_test_bids();
		})
		.then_advance_n_blocks_and_execute_with_checks(EPOCH_DURATION, || {
			assert_eq!(
				ValidatorPallet::current_authorities().into_iter().collect::<BTreeSet<u64>>(),
				genesis_set,
				"we should still be validating with the genesis authorities"
			);

			assert_rotation_phase_matches!(RotationPhase::<Test>::KeygensInProgress(..));
			MockKeyRotatorA::keygen_success();
		})
		.then_advance_n_blocks_and_execute_with_checks(2, || {
			assert_rotation_phase_matches!(RotationPhase::<Test>::KeyHandoversInProgress(..));
			MockKeyRotatorA::key_handover_success();
		})
		.then_advance_n_blocks_and_execute_with_checks(2, || {
			assert_rotation_phase_matches!(RotationPhase::<Test>::ActivatingKeys(..));

			MockKeyRotatorA::keys_activated();
		})
		.then_advance_n_blocks_and_execute_with_checks(2, || {
			assert_default_rotation_outcome!();
		});
}

#[test]
fn genesis() {
	new_test_ext().then_execute_with_checks(|| {
		assert_eq!(
			CurrentAuthorities::<Test>::get().into_iter().collect::<BTreeSet<u64>>(),
			BTreeSet::from(GENESIS_AUTHORITIES),
			"We should have a set of validators at genesis"
		);
		assert_eq!(Bond::<Test>::get(), GENESIS_BOND, "We should have a minimum bid at genesis");
		assert_epoch_index(GENESIS_EPOCH);
	});
}

#[test]
fn send_cfe_version() {
	new_test_ext().then_execute_with_checks(|| {
		// We initially submit version
		let authority = GENESIS_AUTHORITIES[0];

		let version = SemVer { major: 4, ..Default::default() };
		assert_ok!(ValidatorPallet::cfe_version(RuntimeOrigin::signed(authority), version,));

		assert_eq!(
			last_event::<Test>(),
			mock::RuntimeEvent::ValidatorPallet(Event::CFEVersionUpdated {
				account_id: authority,
				old_version: SemVer::default(),
				new_version: version,
			}),
			"should emit event on updated version"
		);

		assert_eq!(
			version,
			ValidatorPallet::node_cfe_version(authority),
			"version should be stored"
		);

		// We submit a new version
		let new_version = SemVer { major: 5, ..Default::default() };
		assert_ok!(ValidatorPallet::cfe_version(RuntimeOrigin::signed(authority), new_version));

		assert_eq!(
			last_event::<Test>(),
			mock::RuntimeEvent::ValidatorPallet(Event::CFEVersionUpdated {
				account_id: authority,
				old_version: version,
				new_version,
			}),
			"should emit event on updated version"
		);

		assert_eq!(
			new_version,
			ValidatorPallet::node_cfe_version(authority),
			"new version should be stored"
		);

		// When we submit the same version we should see no `CFEVersionUpdated` event
		frame_system::Pallet::<Test>::reset_events();
		assert_ok!(ValidatorPallet::cfe_version(RuntimeOrigin::signed(authority), new_version));

		assert_eq!(
			0,
			frame_system::Pallet::<Test>::events().len(),
			"We should have no events of an update"
		);

		assert_eq!(
			new_version,
			ValidatorPallet::node_cfe_version(authority),
			"we should be still on the same new version"
		);
	});
}

#[test]
fn register_peer_id() {
	new_test_ext().then_execute_with_checks(|| {
		use sp_core::{Encode, Pair};

		assert_ok!(<<Test as Chainflip>::AccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(&ALICE));
		assert_ok!(<<Test as Chainflip>::AccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(&BOB));

		let alice_peer_keypair = sp_core::ed25519::Pair::from_legacy_string("alice", None);
		let alice_peer_public_key = alice_peer_keypair.public();

		// Don't allow invalid signatures
		assert_noop!(
			ValidatorPallet::register_peer_id(
				RuntimeOrigin::signed(ALICE),
				alice_peer_public_key,
				0,
				0,
				alice_peer_keypair.sign(&BOB.encode()[..]),
			),
			Error::<Test>::InvalidAccountPeerMappingSignature
		);

		// Non-overlapping peer ids and valid signatures
		assert_ok!(ValidatorPallet::register_peer_id(
			RuntimeOrigin::signed(ALICE),
			alice_peer_public_key,
			40044,
			10,
			alice_peer_keypair.sign(&ALICE.encode()[..]),
		));

		assert_eq!(
			MockCfeInterface::take_events(),
			vec![
			MockCfeEvent::PeerIdRegistered {
				account_id: ALICE,
				pubkey: alice_peer_public_key,
				port: 40044,
				ip: 10
			}],
			"should emit event on register peer id"
		);
		assert_eq!(ValidatorPallet::mapped_peer(&alice_peer_public_key), Some(()));
		assert_eq!(ValidatorPallet::node_peer_id(&ALICE), Some((alice_peer_public_key, 40044, 10)));

		// New mappings to overlapping peer id are disallowed
		assert_noop!(
			ValidatorPallet::register_peer_id(
				RuntimeOrigin::signed(BOB),
				alice_peer_public_key,
				0,
				0,
				alice_peer_keypair.sign(&BOB.encode()[..]),
			),
			Error::<Test>::AccountPeerMappingOverlap
		);

		// New authority mapping works
		let bob_peer_keypair = sp_core::ed25519::Pair::from_legacy_string("bob", None);
		let bob_peer_public_key = bob_peer_keypair.public();
		assert_ok!(ValidatorPallet::register_peer_id(
			RuntimeOrigin::signed(BOB),
			bob_peer_public_key,
			40043,
			11,
			bob_peer_keypair.sign(&BOB.encode()[..]),
		),);

		assert_eq!(
			MockCfeInterface::take_events(),
			vec![
			MockCfeEvent::PeerIdRegistered {
				account_id: BOB,
				pubkey: bob_peer_public_key,
				port: 40043,
				ip: 11
			}],
			"should emit event on register peer id"
		);
		assert_eq!(ValidatorPallet::mapped_peer(&bob_peer_public_key), Some(()));
		assert_eq!(ValidatorPallet::node_peer_id(&BOB), Some((bob_peer_public_key, 40043, 11)));

		// Changing existing mapping to overlapping peer id is disallowed
		assert_noop!(
			ValidatorPallet::register_peer_id(
				RuntimeOrigin::signed(BOB),
				alice_peer_public_key,
				0,
				0,
				alice_peer_keypair.sign(&BOB.encode()[..]),
			),
			Error::<Test>::AccountPeerMappingOverlap
		);

		let bob_peer_keypair = sp_core::ed25519::Pair::from_legacy_string("bob2", None);
		let bob_peer_public_key = bob_peer_keypair.public();

		// Changing to new peer id works
		assert_ok!(ValidatorPallet::register_peer_id(
			RuntimeOrigin::signed(BOB),
			bob_peer_public_key,
			40043,
			11,
			bob_peer_keypair.sign(&BOB.encode()[..]),
		));

		assert_eq!(
			MockCfeInterface::take_events(),
			vec![
			MockCfeEvent::PeerIdRegistered {
				account_id: BOB,
				pubkey: bob_peer_public_key,
				port: 40043,
				ip: 11
			}],
			"should emit event on register peer id"
		);
		assert_eq!(ValidatorPallet::mapped_peer(&bob_peer_public_key), Some(()));
		assert_eq!(ValidatorPallet::node_peer_id(&BOB), Some((bob_peer_public_key, 40043, 11)));

		// Updating only the ip address works
		assert_ok!(ValidatorPallet::register_peer_id(
			RuntimeOrigin::signed(BOB),
			bob_peer_public_key,
			40043,
			12,
			bob_peer_keypair.sign(&BOB.encode()[..]),
		));
		assert_eq!(ValidatorPallet::mapped_peer(&bob_peer_public_key), Some(()));
		assert_eq!(ValidatorPallet::node_peer_id(&BOB), Some((bob_peer_public_key, 40043, 12)));
	});
}

#[test]
fn rerun_auction_if_not_enough_participants() {
	new_test_ext()
		.execute_with(|| {
			// Un-qualify one of the auction winners
			// Change the auction parameters to simulate a shortage in available candidates
			set_default_test_bids();
			let num_bidders = ValidatorPallet::get_active_bids().len() as u32;

			assert_ok!(ValidatorPallet::update_pallet_config(
				RuntimeOrigin::root(),
				PalletConfigUpdate::AuctionParameters {
					parameters: SetSizeParameters {
						min_size: num_bidders + 1,
						max_size: 150,
						max_expansion: 150
					}
				}
			));
		})
		// Run to the epoch boundary
		.then_advance_n_blocks_and_execute_with_checks(EPOCH_DURATION, || {
			cf_test_utilities::assert_has_event::<Test>(RuntimeEvent::ValidatorPallet(
				Event::RotationAborted,
			));
			// Assert that we still in the idle phase
			assert_rotation_phase_matches!(RotationPhase::<Test>::Idle);
			let num_bidders = ValidatorPallet::get_active_bids().len() as u32;
			assert_ok!(ValidatorPallet::update_pallet_config(
				RuntimeOrigin::root(),
				PalletConfigUpdate::AuctionParameters {
					parameters: SetSizeParameters {
						min_size: num_bidders - 1,
						max_size: 150,
						max_expansion: 150
					}
				}
			));
		})
		// Run to the next block - we expect and immediate retry
		.then_advance_n_blocks_and_execute_with_checks(1, || {
			// Expect a resolved auction and kicked-off keygen
			assert_rotation_phase_matches!(RotationPhase::<Test>::KeygensInProgress(..));
		});
}

#[test]
fn historical_epochs() {
	new_test_ext().then_execute_with_checks(|| {
		// Activate an epoch for ALICE
		EpochHistory::<Test>::activate_epoch(&ALICE, 1);
		// Expect the the epoch to be in the storage for ALICE
		assert!(HistoricalActiveEpochs::<Test>::get(ALICE).contains(&1));
		// Activate the next epoch
		EpochHistory::<Test>::activate_epoch(&ALICE, 2);
		// Remove epoch 1 for ALICE
		EpochHistory::<Test>::deactivate_epoch(&ALICE, 1);
		// Expect the epoch to be removed
		assert!(!HistoricalActiveEpochs::<Test>::get(ALICE).contains(&1));
		// and epoch 2 still in storage
		assert!(HistoricalActiveEpochs::<Test>::get(ALICE).contains(&2));
		// Deactivate epoch 2
		EpochHistory::<Test>::deactivate_epoch(&ALICE, 2);
		// And expect the historical active epoch array for ALICE to be empty
		assert!(HistoricalActiveEpochs::<Test>::get(ALICE).is_empty());
	});
}

#[test]
fn expired_epoch_data_is_removed() {
	new_test_ext().then_execute_with_checks(|| {
		// Epoch 1
		EpochHistory::<Test>::activate_epoch(&ALICE, 1);
		HistoricalAuthorities::<Test>::insert(1, Vec::from([ALICE]));
		HistoricalBonds::<Test>::insert(1, 10);
		// Epoch 2
		EpochHistory::<Test>::activate_epoch(&ALICE, 2);
		HistoricalAuthorities::<Test>::insert(2, Vec::from([ALICE]));
		HistoricalBonds::<Test>::insert(2, 30);
		let authority_index = AuthorityIndex::<Test>::get(2, ALICE);

		// Expire
		ValidatorPallet::expire_epoch(1);

		// Epoch 3
		EpochHistory::<Test>::activate_epoch(&ALICE, 3);
		HistoricalAuthorities::<Test>::insert(3, Vec::from([ALICE]));
		HistoricalBonds::<Test>::insert(3, 20);

		// Expect epoch 1's data to be deleted
		assert!(AuthorityIndex::<Test>::try_get(1, ALICE).is_err());
		assert!(HistoricalAuthorities::<Test>::try_get(1).is_err());
		assert!(HistoricalBonds::<Test>::try_get(1).is_err());

		// Expect epoch 2's data to be exist
		assert_eq!(AuthorityIndex::<Test>::get(2, ALICE), authority_index);
		assert_eq!(HistoricalAuthorities::<Test>::get(2), vec![ALICE]);
		assert_eq!(HistoricalBonds::<Test>::get(2), 30);
	});
}

#[test]
fn highest_bond() {
	new_test_ext().then_execute_with_checks(|| {
		// Epoch 1
		EpochHistory::<Test>::activate_epoch(&ALICE, 1);
		HistoricalAuthorities::<Test>::insert(1, Vec::from([ALICE]));
		HistoricalBonds::<Test>::insert(1, 10);
		// Epoch 2
		EpochHistory::<Test>::activate_epoch(&ALICE, 2);
		HistoricalAuthorities::<Test>::insert(2, Vec::from([ALICE]));
		HistoricalBonds::<Test>::insert(2, 30);
		// Epoch 3
		EpochHistory::<Test>::activate_epoch(&ALICE, 3);
		HistoricalAuthorities::<Test>::insert(3, Vec::from([ALICE]));
		HistoricalBonds::<Test>::insert(3, 20);
		// Expect the bond of epoch 2
		assert_eq!(EpochHistory::<Test>::active_bond(&ALICE), 30);
		// Deactivate all epochs
		EpochHistory::<Test>::deactivate_epoch(&ALICE, 1);
		EpochHistory::<Test>::deactivate_epoch(&ALICE, 2);
		EpochHistory::<Test>::deactivate_epoch(&ALICE, 3);
		// Expect the bond to be zero if there is no epoch the node is active in
		assert_eq!(EpochHistory::<Test>::active_bond(&ALICE), 0);
	});
}

#[test]
fn test_missing_author_punishment() {
	let (expected_authority_index, authored_authority_index) = (1usize, 3usize);
	new_test_ext()
		.then_execute_with_checks(|| {
			// Use a large offset to ensure the modulo math selects the correct validators.
			let offset: u64 = GENESIS_AUTHORITIES.len() as u64 * 123456;
			MockMissedAuthorshipSlots::set(
				expected_authority_index as u64 + offset,
				authored_authority_index as u64 + offset,
			);
		})
		.then_advance_n_blocks_and_execute_with_checks(1, || {
			MockOffenceReporter::assert_reported(
				PalletOffence::MissedAuthorshipSlot,
				ValidatorPallet::current_authorities()
					.into_iter()
					.collect::<Vec<_>>()
					.get(expected_authority_index..authored_authority_index)
					.unwrap()
					.to_vec(),
			)
		});
}

#[test]
fn no_validator_rotation_when_disabled_by_safe_mode() {
	new_test_ext().then_execute_with_checks(|| {
		// Activate Safe Mode: CODE RED
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_red();
		assert!(<MockRuntimeSafeMode as Get<PalletSafeMode>>::get() == PalletSafeMode::CODE_RED);

		// Try to start a rotation.
		ValidatorPallet::start_authority_rotation();
		assert_rotation_phase_matches!(RotationPhase::Idle);
		assert_noop!(
			ValidatorPallet::force_rotation(RawOrigin::Root.into()),
			Error::<Test>::RotationsDisabled
		);
		assert_rotation_phase_matches!(RotationPhase::Idle);

		// Change safe mode to CODE GREEN
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_green();
		assert!(<MockRuntimeSafeMode as Get<PalletSafeMode>>::get() == PalletSafeMode::CODE_GREEN);

		// Try to start a rotation.
		set_default_test_bids();
		ValidatorPallet::start_authority_rotation();
		assert_rotation_phase_matches!(RotationPhase::KeygensInProgress(..));
	});
}

#[test]
fn only_governance_can_force_rotation() {
	new_test_ext().then_execute_with_checks(|| {
		assert_noop!(
			ValidatorPallet::force_rotation(OriginTrait::none()),
			sp_runtime::traits::BadOrigin
		);
		assert_ok!(ValidatorPallet::force_rotation(RuntimeOrigin::root()));
	});
}

#[test]
fn test_reputation_is_reset_on_expired_epoch() {
	new_test_ext().execute_with(|| {
		assert!(!MockReputationResetter::<Test>::reputation_was_reset());

		ValidatorPallet::expire_epoch(ValidatorPallet::current_epoch());

		assert!(MockReputationResetter::<Test>::reputation_was_reset());
	});
}
#[cfg(test)]
mod bond_expiry {
	use super::*;

	#[test]
	fn increasing_bond() {
		new_test_ext().execute_with(|| {
			const BOND: u128 = 100;
			let initial_epoch = ValidatorPallet::current_epoch();
			ValidatorPallet::transition_to_next_epoch(vec![1, 2], BOND);
			assert_eq!(ValidatorPallet::bond(), BOND);

			// Ensure the new bond is set for each authority
			ValidatorPallet::current_authorities().iter().for_each(|account_id| {
				assert_eq!(MockBonderFor::<Test>::get_bond(account_id), BOND);
			});

			const NEXT_BOND: u128 = BOND + 1;
			ValidatorPallet::transition_to_next_epoch(vec![2, 3], NEXT_BOND);
			assert_eq!(ValidatorPallet::bond(), NEXT_BOND);

			ValidatorPallet::current_authorities().iter().for_each(|account_id| {
				assert_eq!(MockBonderFor::<Test>::get_bond(account_id), NEXT_BOND);
			});

			assert_eq!(EpochHistory::<Test>::active_epochs_for_authority(&1), [initial_epoch + 1]);
			assert_eq!(EpochHistory::<Test>::active_bond(&1), BOND);
			assert_eq!(
				EpochHistory::<Test>::active_epochs_for_authority(&2),
				[initial_epoch + 1, initial_epoch + 2]
			);
			assert_eq!(EpochHistory::<Test>::active_bond(&2), NEXT_BOND);
			assert_eq!(EpochHistory::<Test>::active_epochs_for_authority(&3), [initial_epoch + 2]);
			assert_eq!(EpochHistory::<Test>::active_bond(&3), NEXT_BOND);
		});
	}

	#[test]
	fn decreasing_bond() {
		new_test_ext().execute_with(|| {
			let initial_epoch = ValidatorPallet::current_epoch();
			const AUTHORITY_IN_BOTH_EPOCHS: u64 = 2;
			ValidatorPallet::transition_to_next_epoch(vec![1, AUTHORITY_IN_BOTH_EPOCHS], 100);
			assert_eq!(ValidatorPallet::bond(), 100);

			ValidatorPallet::current_authorities().iter().for_each(|account_id| {
				assert_eq!(MockBonderFor::<Test>::get_bond(account_id), 100);
			});

			ValidatorPallet::transition_to_next_epoch(vec![AUTHORITY_IN_BOTH_EPOCHS, 3], 99);
			assert_eq!(ValidatorPallet::bond(), 99);

			// Keeps the highest bond of all the epochs it's been active in
			assert_eq!(MockBonderFor::<Test>::get_bond(&AUTHORITY_IN_BOTH_EPOCHS), 100);
			// Uses the new bond
			assert_eq!(MockBonderFor::<Test>::get_bond(&3), 99);

			assert_eq!(EpochHistory::<Test>::active_epochs_for_authority(&1), [initial_epoch + 1]);
			assert_eq!(EpochHistory::<Test>::active_bond(&1), 100);
			assert_eq!(
				EpochHistory::<Test>::active_epochs_for_authority(&2),
				[initial_epoch + 1, initial_epoch + 2]
			);
			assert_eq!(EpochHistory::<Test>::active_bond(&2), 100);
			assert_eq!(EpochHistory::<Test>::active_epochs_for_authority(&3), [initial_epoch + 2]);
			assert_eq!(EpochHistory::<Test>::active_bond(&3), 99);
		});
	}
}

#[test]
fn auction_params_must_be_valid_when_set() {
	new_test_ext().then_execute_with_checks(|| {
		assert_noop!(
			ValidatorPallet::update_pallet_config(
				RuntimeOrigin::root(),
				PalletConfigUpdate::AuctionParameters { parameters: SetSizeParameters::default() }
			),
			Error::<Test>::InvalidAuctionParameters
		);

		assert_ok!(ValidatorPallet::update_pallet_config(
			RuntimeOrigin::root(),
			PalletConfigUpdate::AuctionParameters {
				parameters: SetSizeParameters { min_size: 3, max_size: 10, max_expansion: 10 }
			}
		));
		// Confirm we have an event
		assert_matches!(
			last_event::<Test>(),
			mock::RuntimeEvent::ValidatorPallet(Event::PalletConfigUpdated { .. }),
		);
	});
}

#[test]
fn test_validator_registration_min_balance() {
	new_test_ext().then_execute_with_checks(|| {
		assert_ok!(Pallet::<Test>::register_as_validator(RuntimeOrigin::signed(ALICE),));
	});
}

#[test]
fn test_expect_validator_register_fails() {
	new_test_ext().then_execute_with_checks(|| {
		const ID: u64 = 42;
		assert_ok!(ValidatorPallet::update_pallet_config(
			RawOrigin::Root.into(),
			PalletConfigUpdate::RegistrationBondPercentage {
				percentage: Percent::from_percent(60),
			},
		));
		MockFlip::credit_funds(&ID, Percent::from_percent(40) * GENESIS_BOND);
		// Reduce the set size target to the current authority count.
		assert_ok!(Pallet::<Test>::update_pallet_config(
			RawOrigin::Root.into(),
			PalletConfigUpdate::AuctionParameters {
				parameters: SetSizeParameters {
					min_size: MIN_AUTHORITY_SIZE,
					max_size: <Pallet<Test> as EpochInfo>::current_authority_count(),
					max_expansion: MAX_AUTHORITY_SET_EXPANSION,
				},
			},
		));
		assert_noop!(
			Pallet::<Test>::register_as_validator(RuntimeOrigin::signed(ID),),
			crate::Error::<Test>::NotEnoughFunds
		);
		// Now set it back to the default.
		assert_ok!(Pallet::<Test>::update_pallet_config(
			RawOrigin::Root.into(),
			PalletConfigUpdate::AuctionParameters {
				parameters: SetSizeParameters {
					min_size: MIN_AUTHORITY_SIZE,
					max_size: MAX_AUTHORITY_SIZE,
					max_expansion: MAX_AUTHORITY_SET_EXPANSION,
				},
			},
		));
		// It should be possible to register now since the actual size is below the target.
		assert_ok!(Pallet::<Test>::register_as_validator(RuntimeOrigin::signed(ID)));
		MockFlip::credit_funds(&ID, Percent::from_percent(20) * GENESIS_BOND);
		// Trying to register again passes the funding check but fails for other reasons.
		assert_noop!(
			Pallet::<Test>::register_as_validator(RuntimeOrigin::signed(ID)),
			cf_traits::mocks::account_role_registry::ALREADY_REGISTERED_ERROR,
		);
	});
}

const CANDIDATES: Range<u64> = 4..14;
const AUTHORITIES: Range<u64> = 0..10;

lazy_static::lazy_static! {
	/// How many candidates can fail without preventing us from re-trying keygen
	static ref MAX_ALLOWED_KEYGEN_OFFENDERS: usize = {

		let min_size = std::cmp::max(MIN_AUTHORITY_SIZE, (Percent::one() - DEFAULT_MAX_AUTHORITY_SET_CONTRACTION) * AUTHORITIES.count() as u32);

		CANDIDATES.count().checked_sub(min_size as usize).unwrap()
	};

	/// How many current authorities can fail to leave enough healthy ones to handover the key
	static ref MAX_ALLOWED_SHARING_OFFENDERS: usize = {
		let total = AUTHORITIES.count();
		let needed = success_threshold_from_share_count(total as u32);
		total.checked_sub(needed as usize).unwrap()
	};
}

fn failed_keygen_with_offenders(offenders: impl IntoIterator<Item = u64>) {
	CurrentAuthorities::<Test>::set(AUTHORITIES.collect());
	CurrentRotationPhase::<Test>::put(RotationPhase::KeygensInProgress(
		RuntimeRotationState::<Test>::from_auction_outcome::<Test>(AuctionOutcome {
			winners: CANDIDATES.collect(),
			losers: Default::default(),
			bond: Default::default(),
		}),
	));

	MockKeyRotatorA::failed(offenders);
	Pallet::<Test>::on_initialize(1);
}
#[cfg(test)]
mod keygen {

	use super::*;

	#[test]
	fn restarts_from_keygen_on_keygen_failure() {
		new_test_ext().execute_with(|| {
			// just one node failed
			failed_keygen_with_offenders(CANDIDATES.take(1));
			assert_rotation_phase_matches!(RotationPhase::KeygensInProgress(..));
		});

		new_test_ext().execute_with(|| {
			// many nodes failed, but enough left to try to restart keygen
			failed_keygen_with_offenders(CANDIDATES.take(*MAX_ALLOWED_KEYGEN_OFFENDERS));
			assert_rotation_phase_matches!(RotationPhase::KeygensInProgress(..));
		});
	}

	#[test]
	fn abort_on_keygen_failure_if_too_many_banned() {
		new_test_ext().execute_with(|| {
			// Not enough unbanned nodes left after this failure, so we should abort
			failed_keygen_with_offenders(CANDIDATES.take(*MAX_ALLOWED_KEYGEN_OFFENDERS + 1));
			assert_rotation_aborted();
		});
	}

	#[test]
	fn rotation_aborts_if_candidates_below_min_percentage() {
		new_test_ext().execute_with(|| {
			// Ban half of the candidates:
			let failing_count = CANDIDATES.count() / 2;
			let remaining_count = CANDIDATES.count() - failing_count;

			// We still have enough candidates according to auction resolver parameters:
			assert!(remaining_count > MIN_AUTHORITY_SIZE as usize);

			// But the rotation should be aborted since authority count would drop too much
			// compared to the previous set:
			assert!(
				remaining_count <
					(Percent::one() - DEFAULT_MAX_AUTHORITY_SET_CONTRACTION) *
						AUTHORITIES.count()
			);

			failed_keygen_with_offenders(CANDIDATES.take(failing_count));
			assert_rotation_aborted();
		});
	}
}

#[cfg(test)]
mod key_handover {

	use super::*;

	fn failed_handover_with_offenders(offenders: impl IntoIterator<Item = u64>) {
		CurrentAuthorities::<Test>::set(AUTHORITIES.collect());
		CurrentRotationPhase::<Test>::put(RotationPhase::KeygensInProgress(
			RuntimeRotationState::<Test>::from_auction_outcome::<Test>(AuctionOutcome {
				winners: CANDIDATES.collect(),
				losers: Default::default(),
				bond: Default::default(),
			}),
		));
		MockKeyRotatorA::keygen_success();
		System::reset_events();
		Pallet::<Test>::on_initialize(1);

		assert_rotation_phase_matches!(RotationPhase::KeyHandoversInProgress(..));
		MockKeyRotatorA::failed(offenders);
		System::reset_events();
		Pallet::<Test>::on_initialize(2);
	}

	#[test]
	fn banned_nodes_persist() {
		let non_candidates = AUTHORITIES
			.collect::<BTreeSet<_>>()
			.difference(&CANDIDATES.collect())
			.copied()
			.collect::<Vec<_>>();

		let fails_keygen = non_candidates[0];
		let fails_handover = non_candidates[1];

		new_test_ext()
			.then_execute_at_next_block(|_| {
				// Failed keygen should restart (should have enough non-banned nodes)
				failed_keygen_with_offenders([fails_keygen]);
			})
			.then_execute_at_next_block(|_| {
				assert_rotation_phase_matches!(RotationPhase::KeygensInProgress(..));
			})
			.then_execute_at_next_block(|_| {
				// Successful keygen should transition to handover
				MockKeyRotatorA::keygen_success();
			})
			.then_execute_at_next_block(|_| {
				assert_rotation_phase_matches!(RotationPhase::KeyHandoversInProgress(..));
			})
			.then_execute_at_next_block(|_| {
				// Handover fails with a different non-candidate and will be retried
				MockKeyRotatorA::failed([fails_handover]);
			})
			.then_execute_at_next_block(|_| {
				// Ensure that banned nodes banned during either keygen or handover aren't selected
				if let RotationPhase::KeyHandoversInProgress(state) =
					CurrentRotationPhase::<Test>::get()
				{
					assert_eq!(
						state
							.authority_candidates()
							.intersection(&BTreeSet::from([fails_keygen, fails_handover]))
							.count(),
						0,
						"banned nodes should have been selected"
					)
				} else {
					panic!("unexpected rotation phase: {:?}", CurrentRotationPhase::<Test>::get());
				}
			});
	}

	#[test]
	fn restarts_if_non_candidates_fail() {
		new_test_ext().execute_with(|| {
			// Still enough current authorities available, we should try again.
			failed_handover_with_offenders(AUTHORITIES.take(*MAX_ALLOWED_SHARING_OFFENDERS));

			assert_rotation_phase_matches!(RotationPhase::KeyHandoversInProgress(..));
		});
	}

	#[test]
	fn abort_if_too_many_current_authorities_fail() {
		// TODO: should unban and keep trying instead (see PRO-786)
		new_test_ext().execute_with(|| {
			// Too many current authorities banned, we abort.
			failed_handover_with_offenders(AUTHORITIES.take(*MAX_ALLOWED_SHARING_OFFENDERS + 1));
			assert_rotation_aborted();
		});
	}

	#[test]
	fn restart_from_keygen_if_many_authorities_including_candidates_fail() {
		new_test_ext().execute_with(|| {
			// What matters is that at least one of the candidate fails,
			// so any other offenders don't change the outcome: reverting
			// to keygen.
			let offenders =
				CANDIDATES.take(1).chain(AUTHORITIES.take(*MAX_ALLOWED_SHARING_OFFENDERS + 1));
			failed_handover_with_offenders(offenders);
			assert_rotation_phase_matches!(RotationPhase::KeygensInProgress(..));
		});
	}

	#[test]
	fn restart_from_keygen_if_a_single_candidate_fails() {
		new_test_ext().execute_with(|| {
			// If even one new validator fails, but all old validators were well-behaved,
			// we revert to keygen.
			failed_handover_with_offenders(CANDIDATES.take(1));
			assert_rotation_phase_matches!(RotationPhase::KeygensInProgress(..));
		});
	}
}

#[test]
fn safe_mode_can_aborts_authority_rotation_before_key_handover() {
	new_test_ext().then_execute_with_checks(|| {
		set_default_test_bids();
		ValidatorPallet::start_authority_rotation();

		assert_rotation_phase_matches!(RotationPhase::<Test>::KeygensInProgress(..));

		MockKeyRotatorA::keygen_success();

		System::reset_events();
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_red();
		ValidatorPallet::on_initialize(1);
		assert_rotation_aborted();
	});
}

#[test]
fn safe_mode_does_not_aborts_authority_rotation_after_key_handover() {
	new_test_ext().then_execute_with_checks(|| {
		set_default_test_bids();
		ValidatorPallet::start_authority_rotation();
		MockKeyRotatorA::keygen_success();
		ValidatorPallet::on_initialize(1);
		MockKeyRotatorA::key_handover_success();

		System::reset_events();
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_red();
		ValidatorPallet::on_initialize(1);
		assert_event_sequence!(
			Test,
			RuntimeEvent::ValidatorPallet(Event::RotationPhaseUpdated {
				new_phase: RotationPhase::ActivatingKeys(..)
			}),
		);

		assert_rotation_phase_matches!(RotationPhase::ActivatingKeys(..));
	});
}

#[test]
fn safe_mode_does_not_aborts_authority_rotation_during_key_activation() {
	new_test_ext().then_execute_with_checks(|| {
		set_default_test_bids();
		ValidatorPallet::start_authority_rotation();
		MockKeyRotatorA::keygen_success();
		ValidatorPallet::on_initialize(1);
		MockKeyRotatorA::key_handover_success();
		ValidatorPallet::on_initialize(1);
		MockKeyRotatorA::keys_activated();

		System::reset_events();
		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_red();
		ValidatorPallet::on_initialize(1);
		assert_event_sequence!(
			Test,
			RuntimeEvent::ValidatorPallet(Event::RotationPhaseUpdated {
				new_phase: RotationPhase::NewKeysActivated(..)
			}),
		);
		assert_rotation_phase_matches!(RotationPhase::NewKeysActivated(..));
	});
}

#[test]
fn authority_rotation_can_succeed_after_aborted_by_safe_mode() {
	new_test_ext()
		.then_execute_with_checks(|| {
			set_default_test_bids();
			// Abort authority rotation using Safe Mode.
			ValidatorPallet::start_authority_rotation();
			MockKeyRotatorA::keygen_success();
			<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_red();
		})
		.then_execute_at_next_block(|_| {
			assert_rotation_phase_matches!(RotationPhase::<Test>::Idle);

			// Restart the authority Rotation.
			<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_green();
			ValidatorPallet::start_authority_rotation();
		})
		.then_execute_at_next_block(|_| {
			assert_rotation_phase_matches!(RotationPhase::<Test>::KeygensInProgress(..));

			MockKeyRotatorA::keygen_success();
		})
		.then_execute_at_next_block(|_| {
			assert_rotation_phase_matches!(RotationPhase::<Test>::KeyHandoversInProgress(..));

			MockKeyRotatorA::key_handover_success();
		})
		.then_execute_at_next_block(|_| {
			assert_rotation_phase_matches!(RotationPhase::<Test>::ActivatingKeys(..));

			MockKeyRotatorA::keys_activated();
		})
		.then_advance_n_blocks_and_execute_with_checks(2, || {
			assert_default_rotation_outcome!();
		});
}

#[test]
fn can_calculate_percentage_cfe_at_target_version() {
	new_test_ext().execute_with(|| {
		let initial_version = SemVer { major: 5, minor: 0, patch: 0 };
		let next_version = SemVer { major: 6, minor: 0, patch: 0 };

		// We initially submit version
		let authorities = [0u64, 1u64, 2u64, 3u64, 4u64, 5u64, 6u64, 7u64, 8u64, 9u64];
		authorities.iter().for_each(|id| {
			let _ = ValidatorPallet::register_as_validator(RuntimeOrigin::signed(*id));
			assert_ok!(ValidatorPallet::cfe_version(RuntimeOrigin::signed(*id), initial_version,));
		});
		CurrentAuthorities::<Test>::set(Vec::from(authorities));

		assert_eq!(
			ValidatorPallet::percent_authorities_compatible_with_version(initial_version),
			Percent::from_percent(100)
		);
		assert_eq!(
			ValidatorPallet::percent_authorities_compatible_with_version(next_version),
			Percent::from_percent(0)
		);

		// Update some authorities' version
		let authorities = [0u64, 1u64, 2u64, 3u64, 4u64, 5u64];
		authorities.iter().for_each(|id| {
			assert_ok!(ValidatorPallet::cfe_version(RuntimeOrigin::signed(*id), next_version,));
		});
		assert_eq!(
			ValidatorPallet::percent_authorities_compatible_with_version(initial_version),
			Percent::from_percent(40)
		);
		assert_eq!(
			ValidatorPallet::percent_authorities_compatible_with_version(next_version),
			Percent::from_percent(60)
		);

		// Change authorities
		CurrentAuthorities::<Test>::set(Vec::from(authorities));
		assert_eq!(
			ValidatorPallet::percent_authorities_compatible_with_version(initial_version),
			Percent::from_percent(0)
		);
		assert_eq!(
			ValidatorPallet::percent_authorities_compatible_with_version(next_version),
			Percent::from_percent(100)
		);

		// Version checking ignores `patch`.
		let compatible_version = SemVer { major: 6, minor: 0, patch: 6 };
		assert_eq!(
			ValidatorPallet::percent_authorities_compatible_with_version(compatible_version),
			Percent::from_percent(100)
		);
	});
}

#[test]
fn qualification_by_cfe_version() {
	new_test_ext().execute_with(|| {
		const VALIDATOR: u64 = GENESIS_AUTHORITIES[0];
		// No value reported, no value set:
		assert!(!NodeCFEVersion::<Test>::contains_key(VALIDATOR));
		assert!(!MinimumReportedCfeVersion::<Test>::exists());
		assert!(QualifyByCfeVersion::<Test>::is_qualified(&VALIDATOR));

		assert_ok!(ValidatorPallet::update_pallet_config(
			OriginTrait::root(),
			PalletConfigUpdate::MinimumReportedCfeVersion {
				version: SemVer { major: 0, minor: 1, patch: 0 }
			}
		));
		assert!(!QualifyByCfeVersion::<Test>::is_qualified(&VALIDATOR));

		// Report a version below the minimum:
		assert_ok!(ValidatorPallet::cfe_version(
			RuntimeOrigin::signed(VALIDATOR),
			SemVer { major: 0, minor: 0, patch: 1 }
		));
		assert!(!QualifyByCfeVersion::<Test>::is_qualified(&VALIDATOR));

		// Report a version equal to the minimum:
		assert_ok!(ValidatorPallet::cfe_version(
			RuntimeOrigin::signed(VALIDATOR),
			SemVer { major: 0, minor: 1, patch: 0 }
		));
		assert!(QualifyByCfeVersion::<Test>::is_qualified(&VALIDATOR));

		// Report a version greater than the minimum:
		assert_ok!(ValidatorPallet::cfe_version(
			RuntimeOrigin::signed(VALIDATOR),
			SemVer { major: 0, minor: 1, patch: 1 }
		));
		assert!(QualifyByCfeVersion::<Test>::is_qualified(&VALIDATOR));

		// Report a version bumping the minor version:
		assert_ok!(ValidatorPallet::cfe_version(
			RuntimeOrigin::signed(VALIDATOR),
			SemVer { major: 0, minor: 2, patch: 0 }
		));
		assert!(QualifyByCfeVersion::<Test>::is_qualified(&VALIDATOR));

		// Report a version bumping the major version:
		assert_ok!(ValidatorPallet::cfe_version(
			RuntimeOrigin::signed(VALIDATOR),
			SemVer { major: 1, minor: 0, patch: 0 }
		));
		assert!(QualifyByCfeVersion::<Test>::is_qualified(&VALIDATOR));

		// Raise the minimum:

		assert_ok!(ValidatorPallet::update_pallet_config(
			OriginTrait::root(),
			PalletConfigUpdate::MinimumReportedCfeVersion {
				version: SemVer { major: 1, minor: 0, patch: 0 }
			}
		));
		assert!(QualifyByCfeVersion::<Test>::is_qualified(&VALIDATOR));

		// Raise the minimum again:
		assert_ok!(ValidatorPallet::update_pallet_config(
			OriginTrait::root(),
			PalletConfigUpdate::MinimumReportedCfeVersion {
				version: SemVer { major: 1, minor: 0, patch: 1 }
			}
		));
		assert!(!QualifyByCfeVersion::<Test>::is_qualified(&VALIDATOR));

		// Make sure that only governance can update the config
		assert_noop!(
			ValidatorPallet::update_pallet_config(
				OriginTrait::signed(ALICE),
				PalletConfigUpdate::MinimumReportedCfeVersion {
					version: SemVer { major: 0, minor: 0, patch: 0 }
				}
			),
			sp_runtime::traits::BadOrigin
		);
	});
}

#[test]
fn validator_registration_and_deregistration() {
	new_test_ext().execute_with(|| {
		// Register as validator
		assert_ok!(ValidatorPallet::register_as_validator(RuntimeOrigin::signed(ALICE),));
		assert_ok!(frame_system::Provider::<Test>::created(&ALICE)); // session keys requires a provider ref.
		assert!(!pallet_session::NextKeys::<Test>::contains_key(ALICE));
		assert_ok!(ValidatorPallet::set_keys(
			RuntimeOrigin::signed(ALICE),
			MockSessionKeys::from(UintAuthorityId(ALICE)),
			Default::default(),
		));

		assert!(pallet_session::NextKeys::<Test>::contains_key(ALICE));

		// Deregistration is blocked while the validator is a bidder.
		add_bids(vec![Bid { bidder_id: ALICE, amount: 100 }]);
		assert_noop!(
			ValidatorPallet::deregister_as_validator(RuntimeOrigin::signed(ALICE),),
			Error::<Test>::StillBidding
		);

		// Stop bidding, deregistration should be possible.
		remove_bids(vec![ALICE]);
		assert_ok!(ValidatorPallet::deregister_as_validator(RuntimeOrigin::signed(ALICE),));

		// State should be cleaned up.
		assert!(!pallet_session::NextKeys::<Test>::contains_key(ALICE));
	});
}

#[test]
fn validator_deregistration_after_expired_epoch() {
	new_test_ext().execute_with(|| {
		const RETIRING_VALIDATOR: u64 = GENESIS_AUTHORITIES[0];
		const REMAINING_AUTHORITIES: [u64; 2] = [GENESIS_AUTHORITIES[1], GENESIS_AUTHORITIES[2]];
		const BOND: u128 = 100;

		ValidatorPallet::transition_to_next_epoch(REMAINING_AUTHORITIES.to_vec(), BOND);

		assert_noop!(
			ValidatorPallet::deregister_as_validator(RuntimeOrigin::signed(RETIRING_VALIDATOR),),
			Error::<Test>::StillBidding
		);

		assert_ok!(ValidatorPallet::stop_bidding(RuntimeOrigin::signed(RETIRING_VALIDATOR)));

		assert_noop!(
			ValidatorPallet::deregister_as_validator(RuntimeOrigin::signed(RETIRING_VALIDATOR),),
			Error::<Test>::StillKeyHolder
		);

		ValidatorPallet::transition_to_next_epoch(REMAINING_AUTHORITIES.to_vec(), BOND);
		ValidatorPallet::transition_to_next_epoch(REMAINING_AUTHORITIES.to_vec(), BOND);

		ValidatorPallet::expire_epochs_up_to(
			ValidatorPallet::current_epoch() - 1,
			Weight::from_all(u64::MAX),
		);

		// Now you can deregister
		assert_ok!(ValidatorPallet::deregister_as_validator(RuntimeOrigin::signed(
			RETIRING_VALIDATOR
		),));
	});
}

#[test]
fn test_start_and_stop_bidding() {
	new_test_ext().execute_with(|| {
		MockEpochInfo::add_authorities(ALICE);
		const AMOUNT: u128 = 100;

		MockFlip::credit_funds(&ALICE, AMOUNT);

		// Not yet registered as validator.
		assert_noop!(ValidatorPallet::stop_bidding(RuntimeOrigin::signed(ALICE)), BadOrigin);
		assert_noop!(ValidatorPallet::start_bidding(RuntimeOrigin::signed(ALICE)), BadOrigin);

		assert!(!ValidatorPallet::is_bidding(&ALICE));

		assert_ok!(<<Test as Chainflip>::AccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(&ALICE));

		assert!(!ValidatorPallet::is_bidding(&ALICE));

		assert_noop!(
			ValidatorPallet::stop_bidding(RuntimeOrigin::signed(ALICE)),
			<Error<Test>>::AlreadyNotBidding
		);

		assert!(!ValidatorPallet::is_bidding(&ALICE));

		assert_ok!(ValidatorPallet::start_bidding(RuntimeOrigin::signed(ALICE)));

		assert!(ValidatorPallet::is_bidding(&ALICE));

		assert_noop!(
			ValidatorPallet::start_bidding(RuntimeOrigin::signed(ALICE)),
			<Error<Test>>::AlreadyBidding
		);

		CurrentRotationPhase::<Test>::set(RotationPhase::KeygensInProgress(Default::default()));

		assert_noop!(
			ValidatorPallet::stop_bidding(RuntimeOrigin::signed(ALICE)),
			<Error<Test>>::AuctionPhase
		);
		assert!(ValidatorPallet::is_bidding(&ALICE));

		// Can stop bidding if outside of auction phase
		CurrentRotationPhase::<Test>::set(RotationPhase::Idle);

		assert_ok!(ValidatorPallet::stop_bidding(RuntimeOrigin::signed(ALICE)));
		assert!(!ValidatorPallet::is_bidding(&ALICE));

		assert_event_sequence!(
			Test,
			RuntimeEvent::ValidatorPallet(Event::StartedBidding { account_id: ALICE }),
			RuntimeEvent::ValidatorPallet(Event::StoppedBidding { account_id: ALICE })
		);
	});
}

#[test]
fn can_determine_is_auction_phase() {
	new_test_ext().execute_with(|| {
		// is auction phase if not RotationPhases::Idle
		[
			RotationPhase::KeygensInProgress(Default::default()),
			RotationPhase::KeyHandoversInProgress(Default::default()),
			RotationPhase::ActivatingKeys(Default::default()),
			RotationPhase::NewKeysActivated(Default::default()),
			RotationPhase::SessionRotating(Default::default(), Default::default()),
		]
		.into_iter()
		.for_each(|phase| {
			CurrentRotationPhase::<Test>::set(phase);
			assert!(ValidatorPallet::is_auction_phase());
		});

		CurrentRotationPhase::<Test>::set(RotationPhase::Idle);
		assert!(!ValidatorPallet::is_auction_phase());

		// In Idle phase, must be within certain % of epoch progress.
		CurrentEpochStartedAt::<Test>::set(1_000);
		EpochDuration::<Test>::set(100);
		RedemptionPeriodAsPercentage::<Test>::set(Percent::from_percent(85));

		// First block of auction phase = 1_000 + 100 * 85% = 1085
		System::set_block_number(1084);
		assert!(!ValidatorPallet::is_auction_phase());

		System::set_block_number(1085);
		assert!(ValidatorPallet::is_auction_phase());
	});
}

#[test]
fn redemption_check_works() {
	new_test_ext().execute_with(|| {
		let validator = WINNING_BIDS[0].bidder_id;

		// Not in auction + not bidding = Can redeem
		CurrentRotationPhase::<Test>::set(RotationPhase::Idle);
		ActiveBidder::<Test>::set(Default::default());
		assert_ok!(ValidatorPallet::ensure_can_redeem(&validator));

		// In Auction + not bidding = Can redeem
		CurrentRotationPhase::<Test>::set(RotationPhase::KeygensInProgress(Default::default()));
		assert_ok!(ValidatorPallet::ensure_can_redeem(&validator));

		// Not in Auction + bidding = Can redeem
		CurrentRotationPhase::<Test>::set(RotationPhase::Idle);
		ActiveBidder::<Test>::mutate(|bidders| bidders.insert(validator));
		assert_ok!(ValidatorPallet::ensure_can_redeem(&validator));

		// Auction Phase + bidding = Cannot redeem
		CurrentRotationPhase::<Test>::set(RotationPhase::KeygensInProgress(Default::default()));
		assert_noop!(ValidatorPallet::ensure_can_redeem(&validator), Error::<Test>::StillBidding);
	});
}

#[test]
fn validator_set_change_propagates_to_session_pallet() {
	new_test_ext()
		// Set some new authorities different from the old ones.
		.then_execute_with_checks(|| {
			assert!(
				Pallet::<Test>::current_authorities() ==
					pallet_session::Pallet::<Test>::validators()
			);
			CurrentRotationPhase::put(RotationPhase::<Test>::NewKeysActivated(
				RuntimeRotationState::<Test>::from_auction_outcome::<Test>(AuctionOutcome {
					winners: WINNING_BIDS.map(|bidder| bidder.bidder_id).to_vec(),
					losers: vec![],
					bond: EXPECTED_BOND,
				}),
			));
		})
		// Run until the new epoch.
		.then_process_blocks_until(|_| CurrentRotationPhase::<Test>::get() == RotationPhase::Idle)
		// Do the consistency checks.
		.then_execute_with_checks(|| {});
}

#[test]
fn can_update_all_config_items() {
	new_test_ext().execute_with(|| {
		const NEW_AUCTION_BID_CUTOFF_PERCENTAGE: Percent = Percent::from_percent(10);
		const NEW_REDEMPTION_PERIOD_AS_PERCENTAGE: Percent = Percent::from_percent(10);
		const NEW_REGISTRATION_BOND_PERCENTAGE: Percent = Percent::from_percent(10);
		const NEW_AUTHORITY_SET_MIN_SIZE: u32 = 0;
		const NEW_BACKUP_REWARD_NODE_PERCENTAGE: Percent = Percent::from_percent(10);
		const NEW_EPOCH_DURATION: u32 = 1;
		const NEW_AUCTION_PARAMETERS: SetSizeParameters =
			SetSizeParameters { min_size: 3, max_size: 10, max_expansion: 10 };
		const NEW_MINIMUM_REPORTED_CFE_VERSION: SemVer = SemVer { major: 1, minor: 0, patch: 0 };
		const NEW_MAX_AUTHORITY_SET_CONTRACTION_PERCENTAGE: Percent = Percent::from_percent(10);

		// Check that the default values are different from the new ones
		assert_ne!(AuctionBidCutoffPercentage::<Test>::get(), NEW_AUCTION_BID_CUTOFF_PERCENTAGE);
		assert_ne!(
			RedemptionPeriodAsPercentage::<Test>::get(),
			NEW_REDEMPTION_PERIOD_AS_PERCENTAGE
		);
		assert_ne!(RegistrationBondPercentage::<Test>::get(), NEW_REGISTRATION_BOND_PERCENTAGE);
		assert_ne!(AuthoritySetMinSize::<Test>::get(), NEW_AUTHORITY_SET_MIN_SIZE);
		assert_ne!(BackupRewardNodePercentage::<Test>::get(), NEW_BACKUP_REWARD_NODE_PERCENTAGE);
		assert_ne!(EpochDuration::<Test>::get(), NEW_EPOCH_DURATION as u64);
		assert_ne!(AuctionParameters::<Test>::get(), NEW_AUCTION_PARAMETERS);
		assert_ne!(MinimumReportedCfeVersion::<Test>::get(), NEW_MINIMUM_REPORTED_CFE_VERSION);
		assert_ne!(
			MaxAuthoritySetContractionPercentage::<Test>::get(),
			NEW_MAX_AUTHORITY_SET_CONTRACTION_PERCENTAGE
		);

		// Update all config items
		let updates = vec![
			PalletConfigUpdate::AuctionBidCutoffPercentage {
				percentage: NEW_AUCTION_BID_CUTOFF_PERCENTAGE,
			},
			PalletConfigUpdate::RedemptionPeriodAsPercentage {
				percentage: NEW_REDEMPTION_PERIOD_AS_PERCENTAGE,
			},
			PalletConfigUpdate::RegistrationBondPercentage {
				percentage: NEW_REGISTRATION_BOND_PERCENTAGE,
			},
			PalletConfigUpdate::AuthoritySetMinSize { min_size: NEW_AUTHORITY_SET_MIN_SIZE },
			PalletConfigUpdate::BackupRewardNodePercentage {
				percentage: NEW_BACKUP_REWARD_NODE_PERCENTAGE,
			},
			PalletConfigUpdate::EpochDuration { blocks: NEW_EPOCH_DURATION },
			PalletConfigUpdate::AuctionParameters { parameters: NEW_AUCTION_PARAMETERS },
			PalletConfigUpdate::MinimumReportedCfeVersion {
				version: NEW_MINIMUM_REPORTED_CFE_VERSION,
			},
			PalletConfigUpdate::MaxAuthoritySetContractionPercentage {
				percentage: NEW_MAX_AUTHORITY_SET_CONTRACTION_PERCENTAGE,
			},
		];
		for update in updates {
			assert_ok!(ValidatorPallet::update_pallet_config(OriginTrait::root(), update.clone()));
			// Check that the events were emitted
			System::assert_has_event(RuntimeEvent::ValidatorPallet(Event::PalletConfigUpdated {
				update,
			}));
		}

		// Check that the new values were set
		assert_eq!(AuctionBidCutoffPercentage::<Test>::get(), NEW_AUCTION_BID_CUTOFF_PERCENTAGE);
		assert_eq!(
			RedemptionPeriodAsPercentage::<Test>::get(),
			NEW_REDEMPTION_PERIOD_AS_PERCENTAGE
		);
		assert_eq!(RegistrationBondPercentage::<Test>::get(), NEW_REGISTRATION_BOND_PERCENTAGE);
		assert_eq!(AuthoritySetMinSize::<Test>::get(), NEW_AUTHORITY_SET_MIN_SIZE);
		assert_eq!(BackupRewardNodePercentage::<Test>::get(), NEW_BACKUP_REWARD_NODE_PERCENTAGE);
		assert_eq!(EpochDuration::<Test>::get(), NEW_EPOCH_DURATION as u64);
		assert_eq!(AuctionParameters::<Test>::get(), NEW_AUCTION_PARAMETERS);
		assert_eq!(MinimumReportedCfeVersion::<Test>::get(), NEW_MINIMUM_REPORTED_CFE_VERSION);
		assert_eq!(
			MaxAuthoritySetContractionPercentage::<Test>::get(),
			NEW_MAX_AUTHORITY_SET_CONTRACTION_PERCENTAGE
		);

		// Make sure that only governance can update the config
		assert_noop!(
			ValidatorPallet::update_pallet_config(
				OriginTrait::signed(ALICE),
				PalletConfigUpdate::AuctionBidCutoffPercentage {
					percentage: NEW_AUCTION_BID_CUTOFF_PERCENTAGE,
				}
			),
			sp_runtime::traits::BadOrigin
		);
	});
}

#[test]
fn should_expire_all_previous_epochs() {
	new_test_ext().execute_with(|| {
		const ID: u64 = 1;
		const BOND: u128 = 100;
		ValidatorPallet::transition_to_next_epoch(vec![ID], BOND);
		let first_epoch = ValidatorPallet::current_epoch();
		ValidatorPallet::transition_to_next_epoch(vec![ID], BOND);
		let second_epoch = ValidatorPallet::current_epoch();
		ValidatorPallet::transition_to_next_epoch(vec![ID], BOND);
		let third_epoch = ValidatorPallet::current_epoch();

		assert_eq!(
			HistoricalActiveEpochs::<Test>::get(ID),
			vec![first_epoch, second_epoch, third_epoch]
		);

		ValidatorPallet::expire_epochs_up_to(second_epoch, Weight::from_all(u64::MAX));

		assert_eq!(HistoricalActiveEpochs::<Test>::get(ID), vec![third_epoch]);
	});
}

#[cfg(test)]
mod operator {
	use cf_test_utilities::assert_has_event;

	use super::*;

	#[test]
	fn can_add_and_block_delegator_list_with_allow_default() {
		new_test_ext().execute_with(|| {
			assert_ok!(ValidatorPallet::register_as_operator(
				OriginTrait::signed(ALICE),
				OperatorSettings {
					fee_bps: MIN_OPERATOR_FEE,
					delegation_acceptance: DelegationAcceptance::Allow,
				},
			));
			// Allow BOB (*not* an exception since allow is the default)
			assert_ok!(ValidatorPallet::allow_delegator(OriginTrait::signed(ALICE), BOB));
			assert!(!Exceptions::<Test>::get(ALICE).contains(&BOB));
			assert_ok!(ValidatorPallet::delegate(OriginTrait::signed(BOB), ALICE));
			assert_eq!(DelegationChoice::<Test>::get(BOB), Some(ALICE));

			// Block BOB
			assert_ok!(ValidatorPallet::block_delegator(OriginTrait::signed(ALICE), BOB));
			assert!(Exceptions::<Test>::get(ALICE).contains(&BOB));
			assert!(DelegationChoice::<Test>::get(BOB).is_none());

			// Allow BOB again
			assert_ok!(ValidatorPallet::allow_delegator(OriginTrait::signed(ALICE), BOB));
			assert!(!Exceptions::<Test>::get(ALICE).contains(&BOB));
			assert_event_sequence!(
				Test,
				RuntimeEvent::ValidatorPallet(Event::DelegatorAllowed {
					operator: ALICE,
					delegator: BOB,
				}),
				RuntimeEvent::ValidatorPallet(Event::Delegated { operator: ALICE, delegator: BOB }),
				RuntimeEvent::ValidatorPallet(Event::UnDelegated {
					operator: ALICE,
					delegator: BOB,
				}),
				RuntimeEvent::ValidatorPallet(Event::DelegatorBlocked {
					operator: ALICE,
					delegator: BOB,
				}),
				RuntimeEvent::ValidatorPallet(Event::DelegatorAllowed {
					operator: ALICE,
					delegator: BOB,
				}),
			);
		});
	}

	#[test]
	fn can_allow_and_block_delegator_list_with_deny_default() {
		new_test_ext().execute_with(|| {
			assert_ok!(ValidatorPallet::register_as_operator(
				OriginTrait::signed(ALICE),
				OperatorSettings {
					fee_bps: MIN_OPERATOR_FEE,
					delegation_acceptance: DelegationAcceptance::Deny,
				},
			));

			// BOB cannot delegate by default (not in exceptions list, deny is default)
			assert_noop!(
				ValidatorPallet::delegate(OriginTrait::signed(BOB), ALICE),
				Error::<Test>::DelegatorBlocked
			);
			assert!(!Exceptions::<Test>::get(ALICE).contains(&BOB));
			assert!(DelegationChoice::<Test>::get(BOB).is_none());

			// Allow BOB (add to exceptions list to override deny default)
			assert_ok!(ValidatorPallet::allow_delegator(OriginTrait::signed(ALICE), BOB));
			assert!(Exceptions::<Test>::get(ALICE).contains(&BOB));
			assert_ok!(ValidatorPallet::delegate(OriginTrait::signed(BOB), ALICE));
			assert_eq!(DelegationChoice::<Test>::get(BOB), Some(ALICE));

			// Block BOB again (remove from exceptions list, back to deny default)
			assert_ok!(ValidatorPallet::block_delegator(OriginTrait::signed(ALICE), BOB));
			assert!(!Exceptions::<Test>::get(ALICE).contains(&BOB));
			assert!(DelegationChoice::<Test>::get(BOB).is_none());

			assert_event_sequence!(
				Test,
				RuntimeEvent::ValidatorPallet(Event::DelegatorAllowed {
					operator: ALICE,
					delegator: BOB,
				}),
				RuntimeEvent::ValidatorPallet(Event::Delegated { operator: ALICE, delegator: BOB }),
				RuntimeEvent::ValidatorPallet(Event::UnDelegated {
					operator: ALICE,
					delegator: BOB,
				}),
				RuntimeEvent::ValidatorPallet(Event::DelegatorBlocked {
					operator: ALICE,
					delegator: BOB,
				}),
			);
		});
	}

	#[test]
	fn can_update_operator_settings() {
		new_test_ext().execute_with(|| {
			assert_ok!(ValidatorPallet::register_as_operator(
				OriginTrait::signed(ALICE),
				OPERATOR_SETTINGS
			));
			assert_ok!(ValidatorPallet::update_operator_settings(
				OriginTrait::signed(ALICE),
				OPERATOR_SETTINGS
			));
			assert_eq!(OperatorSettingsLookup::<Test>::get(ALICE), Some(OPERATOR_SETTINGS));
			assert_event_sequence!(
				Test,
				RuntimeEvent::ValidatorPallet(Event::OperatorSettingsUpdated {
					operator: ALICE,
					preferences: OPERATOR_SETTINGS,
				}),
			);
		});
	}
	#[test]
	fn can_claim_by_operator_and_accept_by_validator() {
		const OP_1: u64 = 1001;
		const OP_2: u64 = 1002;
		const V_1: u64 = 2001;
		const V_2: u64 = 2002;

		new_test_ext().execute_with(|| {
			assert_ok!(ValidatorPallet::register_as_operator(
				OriginTrait::signed(OP_1),
				OPERATOR_SETTINGS,
			));
			assert_ok!(ValidatorPallet::register_as_operator(
				OriginTrait::signed(OP_2),
				OPERATOR_SETTINGS,
			));
			assert_ok!(ValidatorPallet::register_as_validator(RuntimeOrigin::signed(V_1),));
			assert_ok!(ValidatorPallet::register_as_validator(RuntimeOrigin::signed(V_2),));

			assert_ok!(ValidatorPallet::claim_validator(OriginTrait::signed(OP_1), V_1));
			assert_eq!(ClaimedValidators::<Test>::get(V_1), [OP_1].into_iter().collect());

			assert_ok!(ValidatorPallet::claim_validator(OriginTrait::signed(OP_1), V_2));
			assert_eq!(ClaimedValidators::<Test>::get(V_1), [OP_1].into_iter().collect());
			assert_eq!(ClaimedValidators::<Test>::get(V_2), [OP_1].into_iter().collect());

			assert_ok!(ValidatorPallet::claim_validator(OriginTrait::signed(OP_2), V_1));
			assert_eq!(ClaimedValidators::<Test>::get(V_1), [OP_1, OP_2].into_iter().collect());
			assert_eq!(ClaimedValidators::<Test>::get(V_2), [OP_1].into_iter().collect());

			// Can't accept operator if validator is not claimed by it.
			assert_noop!(
				ValidatorPallet::accept_operator(OriginTrait::signed(V_2), OP_2),
				Error::<Test>::NotClaimedByOperator
			);

			// Accept operator.
			assert_ok!(ValidatorPallet::accept_operator(OriginTrait::signed(V_2), OP_1));
			assert!(ClaimedValidators::<Test>::get(V_2).is_empty());
			assert_eq!(ClaimedValidators::<Test>::get(V_1), [OP_1, OP_2].into_iter().collect());

			// Can't accept operator if validator is already managed by another operator.
			assert_noop!(
				ValidatorPallet::accept_operator(OriginTrait::signed(V_2), OP_2),
				Error::<Test>::AlreadyManagedByOperator
			);
			assert_ok!(ValidatorPallet::accept_operator(OriginTrait::signed(V_1), OP_2));
			assert_noop!(
				ValidatorPallet::accept_operator(OriginTrait::signed(V_1), OP_1),
				Error::<Test>::AlreadyManagedByOperator
			);

			// Expected end state:
			assert_eq!(ManagedValidators::<Test>::get(V_1), Some(OP_2));
			assert_eq!(ManagedValidators::<Test>::get(V_2), Some(OP_1));

			assert_has_event::<Test>(RuntimeEvent::ValidatorPallet(
				Event::OperatorAcceptedByValidator { validator: V_1, operator: OP_2 },
			));
			assert_has_event::<Test>(RuntimeEvent::ValidatorPallet(
				Event::OperatorAcceptedByValidator { validator: V_2, operator: OP_1 },
			));
		});
	}
	#[test]
	fn validator_and_operator_can_remove_validator() {
		new_test_ext().execute_with(|| {
			ManagedValidators::<Test>::insert(BOB, ALICE);
			// ALICE can remove BOB
			assert_ok!(ValidatorPallet::remove_validator(OriginTrait::signed(ALICE), BOB));
			ManagedValidators::<Test>::insert(BOB, ALICE);
			// BOB can remove BOB
			assert_ok!(ValidatorPallet::remove_validator(OriginTrait::signed(BOB), BOB));
			assert_event_sequence!(
				Test,
				RuntimeEvent::ValidatorPallet(Event::ValidatorRemovedFromOperator {
					validator: BOB,
					operator: ALICE,
				}),
			);
		});
	}
	#[test]
	fn can_not_deregister_if_their_are_still_validators_associated() {
		new_test_ext().execute_with(|| {
			assert_ok!(ValidatorPallet::register_as_operator(
				OriginTrait::signed(ALICE),
				OPERATOR_SETTINGS
			));
			ManagedValidators::<Test>::insert(BOB, ALICE);
			assert_noop!(
				ValidatorPallet::deregister_as_operator(OriginTrait::signed(ALICE)),
				Error::<Test>::StillAssociatedWithValidators
			);
			assert_ok!(ValidatorPallet::remove_validator(OriginTrait::signed(ALICE), BOB));
			assert_ok!(ValidatorPallet::deregister_as_operator(OriginTrait::signed(ALICE)));
		});
	}

	#[test]
	fn exceptions_list_is_reset_when_operator_settings_are_updated() {
		new_test_ext().execute_with(|| {
			assert_ok!(ValidatorPallet::register_as_operator(
				OriginTrait::signed(ALICE),
				OPERATOR_SETTINGS
			));

			Exceptions::<Test>::insert(ALICE, vec![BOB].into_iter().collect::<BTreeSet<_>>());
			assert_ok!(ValidatorPallet::update_operator_settings(
				OriginTrait::signed(ALICE),
				OperatorSettings {
					fee_bps: 300,
					delegation_acceptance: DelegationAcceptance::Deny,
				}
			));
			assert!(Exceptions::<Test>::get(ALICE).is_empty());
		});
	}
}

#[cfg(test)]
mod delegation {
	use super::*;

	#[test]
	fn can_delegate() {
		new_test_ext().execute_with(|| {
			assert_ok!(ValidatorPallet::register_as_operator(
				OriginTrait::signed(BOB),
				OPERATOR_SETTINGS,
			));
			assert_ok!(ValidatorPallet::update_operator_settings(
				OriginTrait::signed(BOB),
				OPERATOR_SETTINGS,
			));
			assert_ok!(ValidatorPallet::delegate(OriginTrait::signed(ALICE), BOB));
			assert_eq!(DelegationChoice::<Test>::get(ALICE), Some(BOB));
			assert_event_sequence!(
				Test,
				RuntimeEvent::ValidatorPallet(Event::OperatorSettingsUpdated {
					operator: BOB,
					preferences: OPERATOR_SETTINGS,
				}),
				RuntimeEvent::ValidatorPallet(Event::Delegated { delegator: ALICE, operator: BOB }),
			);
		});
	}

	#[test]
	fn can_undelegate() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				ValidatorPallet::undelegate(OriginTrait::signed(ALICE)),
				Error::<Test>::AccountIsNotDelegating
			);
			DelegationChoice::<Test>::insert(ALICE, BOB);
			assert_ok!(ValidatorPallet::undelegate(OriginTrait::signed(ALICE)));
			assert_event_sequence!(
				Test,
				RuntimeEvent::ValidatorPallet(Event::UnDelegated {
					delegator: ALICE,
					operator: BOB
				}),
			);
		});
	}

	#[test]
	fn can_not_delegate_if_account_is_blocked() {
		new_test_ext().execute_with(|| {
			assert_ok!(ValidatorPallet::register_as_operator(
				OriginTrait::signed(ALICE),
				OperatorSettings {
					fee_bps: MIN_OPERATOR_FEE,
					delegation_acceptance: DelegationAcceptance::Deny
				},
			));
			assert_noop!(
				ValidatorPallet::delegate(OriginTrait::signed(BOB), ALICE),
				Error::<Test>::DelegatorBlocked
			);
			assert_ok!(ValidatorPallet::allow_delegator(OriginTrait::signed(ALICE), BOB));
			assert_ok!(ValidatorPallet::delegate(OriginTrait::signed(BOB), ALICE),);
		});
	}

	#[test]
	fn can_not_delegate_if_account_is_not_whitelisted() {
		new_test_ext().execute_with(|| {
			assert_ok!(ValidatorPallet::register_as_operator(
				OriginTrait::signed(ALICE),
				OperatorSettings {
					fee_bps: MIN_OPERATOR_FEE,
					delegation_acceptance: DelegationAcceptance::Allow
				},
			));
			assert_ok!(ValidatorPallet::delegate(OriginTrait::signed(BOB), ALICE));
			assert_ok!(ValidatorPallet::undelegate(OriginTrait::signed(BOB)));
			assert_ok!(ValidatorPallet::block_delegator(OriginTrait::signed(ALICE), BOB));
			assert_noop!(
				ValidatorPallet::delegate(OriginTrait::signed(BOB), ALICE),
				Error::<Test>::DelegatorBlocked
			);
		});
	}
}
