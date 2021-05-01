use crate::{mock::*, Stakes, Pallet, Error, PendingClaims, Config};
use frame_support::{assert_err, assert_noop, assert_ok, error::BadOrigin};
use sp_core::ecdsa::Signature;

fn assert_event_sequence<T: frame_system::Config, E: Into<T::Event>>(expected: Vec<E>) 
{
	let events = frame_system::Pallet::<T>::events()
		.into_iter()
		.rev()
		.take(expected.len())
		.rev()
		.map(|e| e.event)
		.collect::<Vec<_>>();
	
	let expected = expected
		.into_iter()
		.map(Into::into)
		.collect::<Vec<_>>();
	
	assert_eq!(events, expected)
}

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
		assert_eq!(Pallet::<Test>::get_total_stake(&ALICE), stake_a1);

		// Add some more
		assert_ok!(StakeManager::staked(Origin::root(), ALICE, stake_a2, ETH_DUMMY_ADDR));
		assert_ok!(StakeManager::staked(Origin::root(), BOB, stake_b, ETH_DUMMY_ADDR));

		// Check storage again.
		assert_eq!(Pallet::<Test>::get_total_stake(&ALICE), stake_a1 + stake_a2);
		assert_eq!(Pallet::<Test>::get_total_stake(&BOB), stake_b);

		// Now claim some FLIP.
		assert_ok!(StakeManager::claim(Origin::signed(ALICE), claim_a, ETH_DUMMY_ADDR));
		assert_ok!(StakeManager::claim(Origin::signed(BOB), claim_b, ETH_DUMMY_ADDR));

		// Make sure it was subtracted.
		assert_eq!(Pallet::<Test>::get_total_stake(&ALICE), stake_a1 + stake_a2 - claim_a);
		assert_eq!(Pallet::<Test>::get_total_stake(&BOB), stake_b - claim_b);

		// Check the pending claims
		assert_eq!(PendingClaims::<Test>::get(ALICE).unwrap().amount, claim_a);
		assert_eq!(PendingClaims::<Test>::get(BOB).unwrap().amount, claim_b);
	});
}

#[test]
fn claiming_unclaimable_is_err() {
	new_test_ext().execute_with(|| {
		let stake = 100_000u128;

		// Claim FLIP before it is staked.
		assert_noop!(
			StakeManager::claim(Origin::signed(ALICE), stake, ETH_DUMMY_ADDR), 
			<Error<Test>>::InsufficientStake
		);

		// Make sure storage hasn't been touched.
		assert_eq!(Stakes::<Test>::contains_key(ALICE), false);

		// Stake some FLIP.
		assert_ok!(StakeManager::staked(Origin::root(), ALICE, stake, ETH_DUMMY_ADDR));

		// assert_event_sequence::<Test, _>(vec![
		// 	crate::Event::Staked(ALICE, stake, stake),
		// ]);

		// Claim FLIP from another account.
		assert_noop!(
			StakeManager::claim(Origin::signed(BOB), stake, ETH_DUMMY_ADDR), 
			<Error<Test>>::InsufficientStake
		);
		
		// Make sure storage hasn't been touched.
		assert_eq!(Pallet::<Test>::get_total_stake(&ALICE), stake);
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
		assert_noop!(
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
		assert_eq!(Pallet::<Test>::get_total_stake(&ALICE), 0u128);
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
		assert_noop!(StakeManager::claimed(Origin::root(), BOB, stake), <Error<Test>>::NoPendingClaim);

		// Invalid Claimed Event from Ethereum: wrong amount.
		assert_noop!(StakeManager::claimed(Origin::root(), ALICE, stake - 1), <Error<Test>>::InvalidClaimAmount);

		// Valid Claimed Event from Ethereum.
		assert_ok!(StakeManager::claimed(Origin::root(), ALICE, stake));
	});
}

#[test]
fn multisig_endpoints_cant_be_called_from_invalid_origins() {
	new_test_ext().execute_with(|| {
		let stake = 1u128;

		assert_noop!(StakeManager::staked(Origin::none(), ALICE, stake, ETH_DUMMY_ADDR), BadOrigin);
		assert_noop!(StakeManager::staked(Origin::signed(Default::default()), ALICE, stake, ETH_DUMMY_ADDR), BadOrigin);

		assert_noop!(StakeManager::claimed(Origin::none(), ALICE, stake), BadOrigin);
		assert_noop!(StakeManager::claimed(Origin::signed(Default::default()), ALICE, stake), BadOrigin);
	});
}

#[test]
fn sigature_is_inserted() {
	new_test_ext().execute_with(|| {
		let stake = 45u128;
		let sig = Signature::from_slice(&[1u8; 65]);

		// Stake some FLIP.
		assert_ok!(StakeManager::staked(Origin::root(), ALICE, stake, ETH_DUMMY_ADDR));

		// Claim it.
		assert_ok!(StakeManager::claim(Origin::signed(ALICE), stake, ETH_DUMMY_ADDR));

		// Check storage for the signature, should not be there.
		assert_eq!(PendingClaims::<Test>::get(ALICE).unwrap().signature, None);
		
		// Insert a signature.
		assert_ok!(StakeManager::post_claim_signature(Origin::signed(ALICE), ALICE, stake, 0, ETH_DUMMY_ADDR, sig.clone()));

		// Check storage for the signature.
		assert_eq!(PendingClaims::<Test>::get(ALICE).unwrap().signature, Some(sig));
	});
}

#[test]
fn witnessing_witnesses() {
	new_test_ext().execute_with(|| {
		witnesser::Mock::set_threshold(2);

		// Bob votes
		assert_ok!(StakeManager::witness_staked(Origin::signed(BOB), ALICE, 123, ETH_DUMMY_ADDR));

		// Should be one vote but not staked yet.
		let count = witnesser::Mock::get_vote_count();
		assert_eq!(count, 1);
		assert_eq!(Pallet::<Test>::get_total_stake(&ALICE), 0);

		// Bob votes again (the mock allows this)
		assert_ok!(StakeManager::witness_staked(Origin::signed(BOB), ALICE, 123, ETH_DUMMY_ADDR));

		// Alice should be staked since we set the threshold to 2.
		assert_eq!(Pallet::<Test>::get_total_stake(&ALICE), 123);
	});
}

#[test]
fn cannot_claim_bond() {
	new_test_ext().execute_with(|| {
		let stake = 200u128;
		bond_provider::Mock::set_bond(100);
		validator_provider::Mock::add_validator(ALICE);

		// Alice and Bob stake the same amount, only alice is a validator. 
		assert_ok!(StakeManager::staked(Origin::root(), ALICE, stake, ETH_DUMMY_ADDR));
		assert_ok!(StakeManager::staked(Origin::root(), BOB, stake, ETH_DUMMY_ADDR));

		// Bob can withdraw all, but not Alice.
		assert_ok!(StakeManager::claim(Origin::signed(BOB), stake, ETH_DUMMY_ADDR));
		assert_noop!(
			StakeManager::claim(Origin::signed(ALICE), stake, ETH_DUMMY_ADDR),
			<Error<Test>>::InsufficientStake
		);

		// Alice *can* withdraw 100
		assert_ok!(StakeManager::claim(Origin::signed(ALICE), 100, ETH_DUMMY_ADDR));

		// Even if she claims, the remaining 100 are blocked
		assert_ok!(StakeManager::claimed(Origin::root(), ALICE, 100));
		assert_noop!(
			StakeManager::claim(Origin::signed(ALICE), 1, ETH_DUMMY_ADDR),
			<Error<Test>>::InsufficientStake
		);
	});
}

#[test]
fn can_signal_retirement() {
	new_test_ext().execute_with(|| {
		validator_provider::Mock::add_validator(ALICE);

		// Need to be staked in order to retire.
		assert_noop!(StakeManager::retire_account(Origin::signed(ALICE)), <Error<Test>>::AccountNotStaked);

		// Try again with some stake, should succeed this time. 
		assert_ok!(StakeManager::staked(Origin::root(), ALICE, 100, ETH_DUMMY_ADDR));
		assert_ok!(StakeManager::retire_account(Origin::signed(ALICE)));

		assert!(StakeManager::is_retired(&ALICE).unwrap());
	});
}