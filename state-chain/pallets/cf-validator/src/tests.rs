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
        assert_ok!(ValidatorManager::set_epoch(Origin::root(), 2));
        assert_eq!(
            last_event(),
            mock::Event::pallet_cf_validator(crate::Event::EpochChanged(0, 2)),
        );

        assert_eq!(ValidatorManager::estimate_next_session_rotation(3), Some(5));
    });
}

#[test]
fn changing_validator_size() {
    new_test_ext().execute_with(|| {
        assert_eq!(<Test as Config>::MinValidatorSetSize::get(), 2);
        assert_noop!(ValidatorManager::set_validator_size(Origin::root(), 0), Error::<Test>::InvalidValidatorSetSize);
        assert_ok!(ValidatorManager::set_validator_size(Origin::root(), 2));
        assert_eq!(
            last_event(),
            mock::Event::pallet_cf_validator(crate::Event::MaximumValidatorsChanged(0, 2)),
        );
        assert_noop!(ValidatorManager::set_validator_size(Origin::root(), 2), Error::<Test>::InvalidValidatorSetSize);
    });
}

#[test]
fn changing_epoch() {
    new_test_ext().execute_with(|| {
        assert_eq!(<Test as Config>::MinEpoch::get(), 1);
        assert_noop!(ValidatorManager::set_epoch(Origin::root(), 0), Error::<Test>::InvalidEpoch);
        assert_ok!(ValidatorManager::set_epoch(Origin::root(), 2));
        assert_eq!(
            last_event(),
            mock::Event::pallet_cf_validator(crate::Event::EpochChanged(0, 2)),
        );
        assert_noop!(ValidatorManager::set_epoch(Origin::root(), 2), Error::<Test>::InvalidEpoch);
    });
}

#[test]
fn sessions_do_end() {
    new_test_ext().execute_with(|| {
        assert!(!ValidatorManager::should_end_session(2));
        assert_ok!(ValidatorManager::set_epoch(Origin::root(), 2));
        assert_eq!(
            last_event(),
            mock::Event::pallet_cf_validator(crate::Event::EpochChanged(0, 2)),
        );
        assert!(ValidatorManager::should_end_session(2));
        assert!(!ValidatorManager::should_end_session(1));
    });
}

#[test]
fn building_a_candidate_list() {
    new_test_ext().execute_with(|| {
        // Pull a list of candidates
        assert_ok!(ValidatorManager::set_validator_size(Origin::root(), 3));
        let maybe_validators = ValidatorManager::run_auction(0).unwrap_or(vec![]);
        assert_eq!(maybe_validators.len(), 3);
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
        assert_noop!(ValidatorManager::set_validator_size(Origin::signed(ALICE), Zero::zero()), BadOrigin);
        assert_noop!(ValidatorManager::force_rotation(Origin::signed(ALICE)), BadOrigin);
    });
}

#[test]
fn bring_forward_session() {
    new_test_ext().execute_with(|| {
        assert_ok!(ValidatorManager::set_validator_size(Origin::root(), 3));
        // Set session epoch to 2, we are on block 1
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
                mock::Event::pallet_cf_validator(crate::Event::AuctionStarted()),
                mock::Event::pallet_cf_validator(crate::Event::AuctionEnded()),
                mock::Event::pallet_session(pallet_session::Event::NewSession(1)),
            ]
        );
        // We have no current validators in first rotation
        assert_eq!(mock::current_validators().len(), 0);
        assert_eq!(mock::next_validators().len(), 3);
        assert_eq!(mock::next_validators()[0], 2);  // Session 2 is next up

        // Move a session forward
        block_number += epoch;
        run_to_block(block_number);
        assert_eq!(
            events(),
            [
                mock::Event::pallet_cf_validator(crate::Event::AuctionStarted()),
                mock::Event::pallet_cf_validator(crate::Event::AuctionEnded()),
                mock::Event::pallet_session(pallet_session::Event::NewSession(2)),
            ]
        );

        assert_eq!(mock::current_validators().len(), 3);
        assert_eq!(mock::current_validators()[0], 2);  // Session 2 is now current
        assert_eq!(mock::next_validators().len(), 3);
        assert_eq!(mock::next_validators()[0], 3);  // Session 3 is now next up

        // Move a session forward
        block_number += epoch;
        run_to_block(block_number);
        assert_eq!(
            events(),
            [
                mock::Event::pallet_cf_validator(crate::Event::AuctionStarted()),
                mock::Event::pallet_cf_validator(crate::Event::AuctionEnded()),
                mock::Event::pallet_session(pallet_session::Event::NewSession(3)),
            ]
        );

        assert_eq!(mock::current_validators().len(), 3);
        assert_eq!(mock::current_validators()[0], 3);  // Session 3 is now current
        assert_eq!(mock::next_validators().len(), 3);
        assert_eq!(mock::next_validators()[0], 4);  // Session 4 is now next up
    });
}

#[test]
fn force_rotation() {
    new_test_ext().execute_with(|| {
        assert_ok!(ValidatorManager::set_validator_size(Origin::root(), 3));
        let epoch = 10;
        let block_number = 2;
        assert_ok!(ValidatorManager::set_epoch(Origin::root(), epoch));
        run_to_block(block_number);
        assert_eq!(mock::next_validators().len(), 0);
        assert_ok!(ValidatorManager::force_rotation(Origin::root()));
        run_to_block(block_number + 1);
        assert_eq!(mock::next_validators().len(), 3);
    });
}

#[test]
fn push_back_session() {
    new_test_ext().execute_with(|| {
        assert_ok!(ValidatorManager::set_validator_size(Origin::root(), 3));
        // Check we get rotation
        let epoch = 2;
        let mut block_number = epoch;
        assert_ok!(ValidatorManager::set_epoch(Origin::root(), epoch));
        run_to_block(block_number);
        assert_eq!(mock::current_validators().len(), 0);
        assert_eq!(mock::next_validators().len(), 3);
        // Push back rotation by an epoch so we should see no rotation now for the last epoch
        assert_ok!(ValidatorManager::set_epoch(Origin::root(), epoch * 2));
        block_number += epoch;
        run_to_block(block_number);
        assert_eq!(mock::current_validators().len(), 0);
        assert_eq!(mock::next_validators().len(), 3);
        // Move forward and now it should rotate
        block_number += epoch;
        run_to_block(block_number);
        assert_eq!(mock::current_validators().len(), 3);
        assert_eq!(mock::next_validators().len(), 3);
    });
}

#[test]
fn limit_validator_set_size() {
    new_test_ext().execute_with(|| {
        // Get current validator size
        // Update (validator size - 1)
        // Force a rotation
        // Confirm we have a (validator - 1) set size

    });
}


