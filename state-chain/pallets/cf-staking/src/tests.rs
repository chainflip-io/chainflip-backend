use crate::{mock::*, Error, Stakes, PendingClaims, Config};
use frame_support::{assert_err, assert_ok, error::BadOrigin};

const ETH_DUMMY_ADDR: <Test as Config>::EthereumAddress = 0u64;

#[test]
fn staked_amount_is_added_and_subtracted() {
	new_test_ext().execute_with(|| {
		let (stake_a1, stake_a2) = (45u128, 21u128);
		let claim_a = 44u128;
		let stake_b = 78u128;
		let claim_b = 78u128;

		// Dispatch a signed extrinsic to stake some FLIP.
		assert_ok!(StakeManager::staked(Origin::root(), ALICE, stake_a1, ETH_DUMMY_ADDR));
		// Read pallet storage and assert the balance was added.
		assert_eq!(Stakes::<Test>::get(ALICE), stake_a1);

		// Add some more
		assert_ok!(StakeManager::staked(Origin::root(), ALICE, stake_a2, ETH_DUMMY_ADDR));
		assert_ok!(StakeManager::staked(Origin::root(), BOB, stake_b, ETH_DUMMY_ADDR));

		// Check storage again.
		assert_eq!(Stakes::<Test>::get(ALICE), stake_a1 + stake_a2);
		assert_eq!(Stakes::<Test>::get(BOB), stake_b);

		// Now claim some FLIP.
		assert_ok!(StakeManager::claim(Origin::signed(ALICE), claim_a, ETH_DUMMY_ADDR));
		assert_ok!(StakeManager::claim(Origin::signed(BOB), claim_b, ETH_DUMMY_ADDR));

		// Make sure it was subtracted.
		assert_eq!(Stakes::<Test>::get(ALICE), stake_a1 + stake_a2 - claim_a);
		assert_eq!(Stakes::<Test>::get(BOB), stake_b - claim_b);

		// Check the pending claims
		assert_eq!(PendingClaims::<Test>::get(ALICE), Some(claim_a));
		assert_eq!(PendingClaims::<Test>::get(BOB), Some(claim_b));
	});
}

#[test]
fn claiming_unclaimable_is_err() {
	new_test_ext().execute_with(|| {
		let stake = 100_000u128;

		// Claim FLIP before it is staked.
		assert_err!(
			StakeManager::claim(Origin::signed(ALICE), stake, ETH_DUMMY_ADDR), 
			<Error<Test>>::InsufficientStake
		);

		// Make sure storage hasn't been touched.
		assert_eq!(Stakes::<Test>::contains_key(ALICE), false);

		// Stake some FLIP.
		assert_ok!(StakeManager::staked(Origin::root(), ALICE, stake, ETH_DUMMY_ADDR));

		// Claim FLIP from another account.
		assert_err!(
			StakeManager::claim(Origin::signed(BOB), stake, ETH_DUMMY_ADDR), 
			<Error<Test>>::InsufficientStake
		);
		
		// Make sure storage hasn't been touched.
		assert_eq!(Stakes::<Test>::get(ALICE), stake);
	});
}

#[test]
fn cannot_double_claim() {
	new_test_ext().execute_with(|| {
		let (stake_a1, stake_a2) = (45u128, 21u128);

		// Stake some FLIP.
		assert_ok!(StakeManager::staked(Origin::root(), ALICE, stake_a1 + stake_a2, ETH_DUMMY_ADDR));

		// Claim a portion.
		assert_ok!(StakeManager::claim(Origin::signed(ALICE), stake_a1, ETH_DUMMY_ADDR));

		// Claiming the rest should not be possible yet.
		assert_err!(
			StakeManager::claim(Origin::signed(ALICE), stake_a2, ETH_DUMMY_ADDR),
			<Error<Test>>::PendingClaim
		);

		// Redeem the first claim.
		assert_ok!(StakeManager::claimed(Origin::root(), ALICE, stake_a1));

		// Should now be able to claim the rest.
		assert_ok!(StakeManager::claim(Origin::signed(ALICE), stake_a2, ETH_DUMMY_ADDR));

		// Redeem the rest.
		assert_ok!(StakeManager::claimed(Origin::root(), ALICE, stake_a2));

		// Remaining stake should be zero
		assert_eq!(Stakes::<Test>::get(ALICE), 0u128);
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