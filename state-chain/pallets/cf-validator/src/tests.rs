use std::{collections::HashSet, vec};

use super::*;
use crate::{Error, mock::*};
use sp_runtime::traits::{BadOrigin, Zero};
use frame_support::{assert_ok, assert_noop};

// Constants
const ALICE: u64 = 100;

fn events() -> Vec<mock::Event> {
	let evt = System::events().into_iter().map(|evt| evt.event).collect::<Vec<_>>();
	System::reset_events();
	evt
}

fn last_event() -> mock::Event {
	frame_system::Pallet::<Test>::events().pop().expect("Event expected").event
}

#[test]
fn estimation_on_next_session() {
	new_test_ext().execute_with(|| {
		// Set epoch to 2 blocks
		assert_ok!(ValidatorManager::set_epoch(Origin::root(), 2));
		// Confirm we have the event of the change to 2
		assert_eq!(
			last_event(),
			mock::Event::pallet_cf_validator(crate::Event::EpochChanged(0, 2)),
		);
		// Simple math to confirm we can work out 3 plus 2
		assert_eq!(ValidatorManager::estimate_next_session_rotation(3), Some(5));
	});
}

#[test]
fn changing_validator_size() {
	new_test_ext().execute_with(|| {
		// Assert our minimum is set to 2
		assert_eq!(<Test as Config>::MinValidatorSetSize::get(), 2);
		// Check we are throwing up an error when we send anything less than the minimum of 2
		assert_noop!(ValidatorManager::set_validator_target_size(Origin::root(), 0), Error::<Test>::InvalidValidatorSetSize);
		assert_noop!(ValidatorManager::set_validator_target_size(Origin::root(), 1), Error::<Test>::InvalidValidatorSetSize);
		// This should now work
		assert_ok!(ValidatorManager::set_validator_target_size(Origin::root(), 2));
		// Confirm we have an event with the change of 0 to 2
		assert_eq!(
			last_event(),
			mock::Event::pallet_cf_validator(crate::Event::MaximumValidatorsChanged(0, 2)),
		);
		// We throw up an error if we try to set it to the current
		assert_noop!(ValidatorManager::set_validator_target_size(Origin::root(), 2), Error::<Test>::InvalidValidatorSetSize);
	});
}

#[test]
fn changing_epoch() {
	new_test_ext().execute_with(|| {
		// Confirm we have a minimum epoch of 1 block
		assert_eq!(<Test as Config>::MinEpoch::get(), 1);
		// Throw up an error if we supply anything less than this
		assert_noop!(ValidatorManager::set_epoch(Origin::root(), 0), Error::<Test>::InvalidEpoch);
		// This should work as 2 > 1
		assert_ok!(ValidatorManager::set_epoch(Origin::root(), 2));
		// Confirm we have an event for the change from 0 to 2
		assert_eq!(
			last_event(),
			mock::Event::pallet_cf_validator(crate::Event::EpochChanged(0, 2)),
		);
		// We throw up an error if we try to set it to the current
		assert_noop!(ValidatorManager::set_epoch(Origin::root(), 2), Error::<Test>::InvalidEpoch);
	});
}

#[test]
fn sessions_do_end() {
	new_test_ext().execute_with(|| {
		// As our epoch is 0 at genesis we should return false always
		assert!(!ValidatorManager::should_end_session(1));
		assert!(!ValidatorManager::should_end_session(2));
		// Set epoch to 2 blocks
		assert_ok!(ValidatorManager::set_epoch(Origin::root(), 2));
		// Confirm we have the event for the change from 0 to 2
		assert_eq!(
			last_event(),
			mock::Event::pallet_cf_validator(crate::Event::EpochChanged(0, 2)),
		);
		// We should now be able to end a session on block 2
		assert!(ValidatorManager::should_end_session(2));
		// This isn't the case for block 1
		assert!(!ValidatorManager::should_end_session(1));
	});
}

#[test]
fn building_a_candidate_list() {
	new_test_ext().execute_with(|| {
		// We are after 3 validators, the mock is set up for 3
		assert_ok!(ValidatorManager::set_validator_target_size(Origin::root(), 3));
		let mut candidates: Vec<(u64, u64)> = vec![(1, 2), (2, 3), (3, 4)];
		let winners: HashSet<u64> = [1, 2, 3].iter().cloned().collect();

		// Run an auction and get our candidate validators, should be 3
		let maybe_validators = ValidatorManager::run_auction(candidates.clone());
		assert_eq!(maybe_validators.map(|v| v.iter().cloned().collect()), Some(winners.clone()));

		// Add a low bid, should not change the validator set.
		candidates.push((4, 1));
		let maybe_validators = ValidatorManager::run_auction(candidates.clone());
		assert_eq!(maybe_validators.map(|v| v.iter().cloned().collect()), Some(winners.clone()));

		// Add a high bid, should alter the winners
		candidates.push((5, 5));
		let winners: HashSet<u64> = [2, 3, 5].iter().cloned().collect();
		let maybe_validators = ValidatorManager::run_auction(candidates.clone());
		assert_eq!(maybe_validators.map(|v| v.iter().cloned().collect()), Some(winners.clone()));
	});
}

#[test]
fn have_optional_validators_on_genesis() {
	new_test_ext().execute_with(|| {
		// Add two validators at genesis
		// Confirm we have them from block 1 in the validator set
	});
}

#[test]
fn you_have_to_be_priviledged() {
	new_test_ext().execute_with(|| {
		// Run through the sudo extrinsics to be sure they are what they are
		assert_noop!(ValidatorManager::set_epoch(Origin::signed(ALICE), Zero::zero()), BadOrigin);
		assert_noop!(ValidatorManager::set_validator_target_size(Origin::signed(ALICE), Zero::zero()), BadOrigin);
		assert_noop!(ValidatorManager::force_rotation(Origin::signed(ALICE)), BadOrigin);
	});
}

#[test]
fn bring_forward_session() {
	new_test_ext().execute_with(|| {
		// We are after 3 validators, the mock is set up for 3
		assert_ok!(ValidatorManager::set_validator_target_size(Origin::root(), 3));
		// Set session epoch to 2
		let epoch = 2;
		let mut block_number = epoch;
		assert_ok!(ValidatorManager::set_epoch(Origin::root(), epoch));
		assert_eq!(mock::current_validators().len(), 0);
		// Move a session forward
		run_to_block(block_number);
		assert_eq!(
			events(),
			[
				mock::Event::pallet_cf_validator(crate::Event::MaximumValidatorsChanged(0, 3)),
				mock::Event::pallet_cf_validator(crate::Event::EpochChanged(0, 2)),
				mock::Event::pallet_cf_validator(crate::Event::AuctionStarted(2)),
				mock::Event::pallet_cf_validator(crate::Event::AuctionEnded(2)),
				mock::Event::pallet_session(pallet_session::Event::NewSession(1)),
			]
		);
		// We have no current validators in first rotation
		assert_eq!(mock::current_validators().len(), 0);
		assert_eq!(mock::next_validators().len(), 0);
		assert_eq!(TestValidatorHandler::get_current_epoch(), 0);

		// Validator set hasn't changed.
		assert_eq!(mock::current_validators(), mock::next_validators());

		// Add another candidate with a higher bid.
		CANDIDATES.with(|cell| cell.borrow_mut().push((4, 4)));
		CANDIDATES.with(|cell| cell.borrow_mut().push((5, 0)));

		// Move a session forward
		block_number += epoch;
		run_to_block(block_number);
		assert_eq!(
			events(),
			[
				mock::Event::pallet_cf_validator(crate::Event::AuctionStarted(3)),
				mock::Event::pallet_cf_validator(crate::Event::AuctionEnded(3)),
				mock::Event::pallet_session(pallet_session::Event::NewSession(2)),
			]
		);

		assert_eq!(mock::current_validators().len(), 3);
		assert_eq!(TestValidatorHandler::get_current_epoch(), 1);
		assert_eq!(mock::next_validators().len(), 3);
		// Validator set change has been queued
		assert_ne!(mock::current_validators(), mock::next_validators());
		// assert_eq!(mock::next_validators()[0], 3);  // Session 3 is now next up

		// Move a session forward
		block_number += epoch;
		run_to_block(block_number);
		assert_eq!(
			events(),
			[
				mock::Event::pallet_cf_validator(crate::Event::AuctionStarted(4)),
				mock::Event::pallet_cf_validator(crate::Event::AuctionEnded(4)),
				mock::Event::pallet_session(pallet_session::Event::NewSession(3)),
			]
		);

		assert_eq!(mock::current_validators().len(), 3);
		assert_eq!(TestValidatorHandler::get_current_epoch(), 2);
		assert_eq!(mock::next_validators().len(), 3);
		// No change in validators. 
		assert_eq!(mock::current_validators(), mock::next_validators());
		// assert_eq!(mock::next_validators()[0], 4);  // Session 4 is now next up
	});
}

#[test]
fn force_rotation() {
	new_test_ext().execute_with(|| {
		// We are after 3 validators, the mock is set up for 3
		assert_ok!(ValidatorManager::set_validator_target_size(Origin::root(), 3));
		assert_eq!(
			last_event(),
			mock::Event::pallet_cf_validator(crate::Event::MaximumValidatorsChanged(0, 3)),
		);
		// Set the epoch at 10
		let epoch = 10;
		let block_number = 2;
		assert_ok!(ValidatorManager::set_epoch(Origin::root(), epoch));
		assert_eq!(
			last_event(),
			mock::Event::pallet_cf_validator(crate::Event::EpochChanged(0, 10)),
		);
		// Clear the event queue
		System::reset_events();
		// Run forward 2 blocks
		run_to_block(block_number);
		// No rotation, no candidates
		assert_eq!(mock::next_validators().len(), 0);
		// Force rotation for next block
		assert_ok!(ValidatorManager::force_rotation(Origin::root()));
		run_to_block(block_number + 1);
		assert_eq!(
			events(),
			[
				mock::Event::pallet_cf_validator(crate::Event::ForceRotationRequested()),
				mock::Event::pallet_cf_validator(crate::Event::AuctionStarted(2)),
				mock::Event::pallet_cf_validator(crate::Event::AuctionEnded(2)),
				mock::Event::pallet_session(pallet_session::Event::NewSession(1)),
			]
		);
		// Hello there candidates
		assert_eq!(mock::next_validators().len(), 3);
	});
}

#[test]
fn push_back_session() {
	new_test_ext().execute_with(|| {
		// We are after 3 validators, the mock is set up for 3
		assert_ok!(ValidatorManager::set_validator_target_size(Origin::root(), 3));
		assert_eq!(
			last_event(),
			mock::Event::pallet_cf_validator(crate::Event::MaximumValidatorsChanged(0, 3)),
		);
		// Check we get rotation
		let epoch = 2;
		let mut block_number = epoch;
		assert_ok!(ValidatorManager::set_epoch(Origin::root(), epoch));
		assert_eq!(
			last_event(),
			mock::Event::pallet_cf_validator(crate::Event::EpochChanged(0, epoch)),
		);
		run_to_block(block_number);
		assert_eq!(mock::current_validators().len(), 0);
		assert_eq!(mock::next_validators().len(), 3);
		// Push back rotation by an epoch so we should see no rotation now for the last epoch
		assert_ok!(ValidatorManager::set_epoch(Origin::root(), epoch * 2));
		assert_eq!(
			last_event(),
			mock::Event::pallet_cf_validator(crate::Event::EpochChanged(epoch, epoch * 2)),
		);
		block_number += epoch;
		run_to_block(block_number);
		assert_eq!(mock::current_validators().len(), 0);
		assert_eq!(mock::next_validators().len(), 3);
		// Clear the event queue
		System::reset_events();
		// Move forward and now it should rotate
		block_number += epoch;
		run_to_block(block_number);
		assert_eq!(mock::current_validators().len(), 3);
		assert_eq!(mock::next_validators().len(), 3);
		assert_eq!(
			events(),
			[
				mock::Event::pallet_cf_validator(crate::Event::AuctionStarted(3)),
				mock::Event::pallet_cf_validator(crate::Event::AuctionEnded(3)),
				mock::Event::pallet_session(pallet_session::Event::NewSession(2)),
			]
		);
	});
}

#[test]
fn limit_validator_set_size() {
	new_test_ext().execute_with(|| {
		// We are after 3 validators, the mock is set up for 3
		assert_ok!(ValidatorManager::set_validator_target_size(Origin::root(), 3));
		assert_eq!(
			last_event(),
			mock::Event::pallet_cf_validator(crate::Event::MaximumValidatorsChanged(0, 3)),
		);
		// Run a rotation
		let epoch = 2;
		let mut block_number = epoch;
		assert_ok!(ValidatorManager::set_epoch(Origin::root(), epoch));
		assert_eq!(
			last_event(),
			mock::Event::pallet_cf_validator(crate::Event::EpochChanged(0, epoch)),
		);
		// Clear the event queue
		System::reset_events();
		run_to_block(block_number);
		assert_eq!(mock::current_validators().len(), 0);
		assert_eq!(mock::next_validators().len(), 3);
		assert_eq!(
			events(),
			[
				mock::Event::pallet_cf_validator(crate::Event::AuctionStarted(2)),
				mock::Event::pallet_cf_validator(crate::Event::AuctionEnded(2)),
				mock::Event::pallet_session(pallet_session::Event::NewSession(1)),
			]
		);
		// Reduce size of validator set, we should see next set of candidates reduced from 3 to 2
		assert_ok!(ValidatorManager::set_validator_target_size(Origin::root(), 2));
		block_number += epoch;
		run_to_block(block_number);
		assert_eq!(mock::current_validators().len(), 3);
		assert_eq!(mock::next_validators().len(), 2);
		assert_eq!(
			events(),
			[
				mock::Event::pallet_cf_validator(crate::Event::MaximumValidatorsChanged(3, 2)),
				mock::Event::pallet_cf_validator(crate::Event::AuctionStarted(3)),
				mock::Event::pallet_cf_validator(crate::Event::AuctionEnded(3)),
				mock::Event::pallet_session(pallet_session::Event::NewSession(2)),
			]
		);
		// One more to see the rotation maintain the new set size of 2
		block_number += epoch;
		run_to_block(block_number);
		assert_eq!(mock::current_validators().len(), 2);
		assert_eq!(mock::next_validators().len(), 2);
		assert_eq!(
			events(),
			[
				mock::Event::pallet_cf_validator(crate::Event::AuctionStarted(4)),
				mock::Event::pallet_cf_validator(crate::Event::AuctionEnded(4)),
				mock::Event::pallet_session(pallet_session::Event::NewSession(3)),
			]
		);
	});
}
