use super::*;
use crate::{Error, mock::*};
use sp_runtime::traits::{BadOrigin, Zero};
use frame_support::{assert_ok, assert_noop};

// Constants
const ALICE: u64 = 100;

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
        let maybe_validators = ValidatorManager::get_validators().unwrap_or(vec![]);
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
        // Set session epoch to 2, we are on block 1
        assert_ok!(ValidatorManager::set_epoch(Origin::root(), 2));
        assert_eq!(
            last_event(),
            mock::Event::pallet_cf_validator(crate::Event::EpochChanged(0, 2)),
        );
        // Move two blocks forward
        run_to_block(2);

    });
}

#[test]
fn push_back_session() {
    new_test_ext().execute_with(|| {
        // Get current next session block number
        // Update next session (block number + 1)
        // Wait (block number + 1) blocks
        // Confirm we had a switch
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

#[test]
fn force_rotation() {
    new_test_ext().execute_with(|| {
        // Force rotation
        // Get validator size
        // Check it has rotated with the set validator size
    });
}
