use crate::{mock::*, Error};
use frame_support::{assert_err, assert_ok, storage::StorageMap, assert_noop};
use sp_runtime::traits::{BadOrigin, Zero};
const ALICE: u64 = 100;

#[test]
fn sessions_do_end() {
    new_test_ext().execute_with(|| {
        assert!(!RotationManager::should_end_session(2));
        // Set blocks for epoch
        assert_ok!(RotationManager::set_epoch(Origin::root(), 2));
        assert!(RotationManager::should_end_session(2));
        assert!(!RotationManager::should_end_session(3));
    });
}

#[test]
fn building_a_candidate_list() {
    new_test_ext().execute_with(|| {
        // Pull a list of candidates from cf-staking
        assert_ok!(RotationManager::get_validators());
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
        assert_noop!(RotationManager::set_epoch(Origin::signed(ALICE), Zero::zero()), BadOrigin);
        assert_noop!(RotationManager::set_validator_size(Origin::signed(ALICE), Zero::zero()), BadOrigin);
        assert_noop!(RotationManager::force_rotation(Origin::signed(ALICE)), BadOrigin);
    });
}

#[test]
fn bring_forward_era() {
    new_test_ext().execute_with(|| {
        // Get current next era block number
        // Update next era (block number - 1)
        // Wait (block number - 1) blocks
        // Confirm things have switched
    });
}

#[test]
fn push_back_era() {
    new_test_ext().execute_with(|| {
        // Get current next era block number
        // Update next era (block number + 1)
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
