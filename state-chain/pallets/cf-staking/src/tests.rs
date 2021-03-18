use crate::{mock::*, Error, Stakes};
use frame_support::{assert_err, assert_ok, storage::StorageMap};

#[test]
fn staked_amount_is_added_and_subtracted() {
    new_test_ext().execute_with(|| {
        // Dispatch a signed extrinsic to stake some FLIP.
        assert_ok!(StakeManager::staked(Origin::none(), 123, 45u128));
        // Read pallet storage and assert the balance was added.
        assert_eq!(Stakes::<Test>::get(123), 45u128);

        // Add some more
        assert_ok!(StakeManager::staked(Origin::none(), 123, 21u128));
        assert_ok!(StakeManager::staked(Origin::none(), 456, 78u128));

        // Check storage again.
        assert_eq!(Stakes::<Test>::get(123), 66u128);
        assert_eq!(Stakes::<Test>::get(456), 78u128);

        // Now claim some FLIP.
        assert_ok!(StakeManager::claimed(Origin::none(), 123, 44u128));
        assert_ok!(StakeManager::claimed(Origin::none(), 456, 78u128));

        // Make sure it was subtracted.
        assert_eq!(Stakes::<Test>::get(456), 0u128);
        assert_eq!(Stakes::<Test>::get(123), 22u128);
    });
}

#[test]
fn staker_with_zero_stake_is_removed() {
    new_test_ext().execute_with(|| {
        // Stake some FLIP.
        assert_ok!(StakeManager::staked(Origin::none(), 123, 45u128));

        // Claim the FLIP.
        assert_ok!(StakeManager::claimed(Origin::none(), 123, 45u128));

        // Make sure the account is removed.
        assert!(!Stakes::<Test>::contains_key(456));
    });
}

#[test]
fn claiming_unclaimable_is_err() {
    new_test_ext().execute_with(|| {
        // Claim the FLIP.
        assert_err!(StakeManager::claimed(Origin::none(), 123, 45u128), <Error<Test>>::UnknownClaimant);
        assert_eq!(Stakes::<Test>::contains_key(123), false);
    });
}

#[test]
fn claiming_too_much_removes_claimant_and_is_err() {
    new_test_ext().execute_with(|| {
        // Stake some FLIP.
        assert_ok!(StakeManager::staked(Origin::none(), 123, 45u128));
        // Claim too much FLIP.
        assert_err!(StakeManager::claimed(Origin::none(), 123, 46u128), <Error<Test>>::ExcessFundsClaimed);
        // Ensure the Id has been removed.
        assert_eq!(Stakes::<Test>::contains_key(123), false);
    });
}