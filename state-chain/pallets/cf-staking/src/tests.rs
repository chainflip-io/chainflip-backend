use crate::{mock::*, Error, Stakes, PendingClaims, Config};
use frame_support::{assert_err, assert_ok, error::BadOrigin};

const ETH_DUMMY_ADDR: <Test as Config>::EthereumAddress = 0u64;

#[test]
fn staked_amount_is_added_and_subtracted() {
    new_test_ext().execute_with(|| {
        // Dispatch a signed extrinsic to stake some FLIP.
        assert_ok!(StakeManager::staked(Origin::root(), ALICE, 45u128, ETH_DUMMY_ADDR));
        // Read pallet storage and assert the balance was added.
        assert_eq!(Stakes::<Test>::get(ALICE), 45u128);

        // Add some more
        assert_ok!(StakeManager::staked(Origin::root(), ALICE, 21u128, ETH_DUMMY_ADDR));
        assert_ok!(StakeManager::staked(Origin::root(), BOB, 78u128, ETH_DUMMY_ADDR));

        // Check storage again.
        assert_eq!(Stakes::<Test>::get(ALICE), 66u128);
        assert_eq!(Stakes::<Test>::get(BOB), 78u128);

        // Now claim some FLIP.
        assert_ok!(StakeManager::claim(Origin::signed(ALICE), 44u128, ETH_DUMMY_ADDR));
        assert_ok!(StakeManager::claim(Origin::signed(BOB), 78u128, ETH_DUMMY_ADDR));

        // Make sure it was subtracted.
        assert_eq!(Stakes::<Test>::get(ALICE), 22u128);
        assert_eq!(Stakes::<Test>::get(BOB), 0u128);

        // Check the pending claims
        assert_eq!(PendingClaims::<Test>::get(ALICE), Some(44u128));
        assert_eq!(PendingClaims::<Test>::get(BOB), Some(78u128));
    });
}

#[test]
fn claiming_unclaimable_is_err() {
    new_test_ext().execute_with(|| {
        // Claim FLIP that doesn't exist.
        assert_err!(
            StakeManager::claim(Origin::signed(ALICE), 45u128, ETH_DUMMY_ADDR), 
            <Error<Test>>::InsufficientStake);

        // Make sure storage hasn't been touched.
        assert_eq!(Stakes::<Test>::contains_key(ALICE), false);
    });
}

#[test]
fn staked_and_claimed_events_must_match() {
    new_test_ext().execute_with(|| {
        let stake = 45u128;

        // Stake some FLIP.
        assert_ok!(StakeManager::staked(Origin::root(), ALICE, stake, ETH_DUMMY_ADDR));

        // Claim it.
        assert_ok!(StakeManager::claim(Origin::signed(ALICE), stake, ETH_DUMMY_ADDR));

        // Invalid Claimed Event from Ethereum: wrong account.
        assert_err!(StakeManager::claimed(Origin::root(), BOB, stake), <Error<Test>>::NoPendingClaim);

        // Invalid Claimed Event from Ethereum: wrong amount.
        assert_err!(StakeManager::claimed(Origin::root(), ALICE, stake - 1), <Error<Test>>::InvalidClaimAmount);

        // Valid Claimed Event from Ethereum.
        assert_ok!(StakeManager::claimed(Origin::root(), ALICE, stake));
    });
}

#[test]
fn multisig_endpoints_cant_be_called_from_invalid_origins() {
    new_test_ext().execute_with(|| {
        let stake = 1u128;

        assert_err!(StakeManager::staked(Origin::none(), ALICE, stake, ETH_DUMMY_ADDR), BadOrigin);
        assert_err!(StakeManager::staked(Origin::signed(Default::default()), ALICE, stake, ETH_DUMMY_ADDR), BadOrigin);

        assert_err!(StakeManager::claimed(Origin::none(), ALICE, stake), BadOrigin);
        assert_err!(StakeManager::claimed(Origin::signed(Default::default()), ALICE, stake), BadOrigin);
    });
}