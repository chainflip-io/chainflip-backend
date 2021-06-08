use crate::{mock::*, Pallet, Error, PendingClaims, Config};
use std::time::Duration;
use frame_support::{assert_noop, assert_ok, error::BadOrigin, traits::UnixTime};
use sp_core::ecdsa::Signature;
use cf_traits::mocks::epoch_info;
use pallet_cf_flip::ImbalanceSource;

type FlipError = pallet_cf_flip::Error<Test>;
type FlipEvent = pallet_cf_flip::Event<Test>;

const ETH_DUMMY_ADDR: <Test as Config>::EthereumAddress = [42u8; 20];

fn time_after<T: Config>(duration: Duration) -> Duration {
	<T::TimeSource as UnixTime>::now() + duration
}

/// Checks the deposited events, in reverse order (reverse order mainly because it makes the macro easier to write).
macro_rules! assert_event_stack {
	($($pat:pat $( => $test:block )? ),*) => {
		let mut events = frame_system::Pallet::<Test>::events()
			.into_iter()
			.map(|e| e.event)
			.collect::<Vec<_>>();

		$(
			let actual = events.pop().expect("Expected an event.");
			#[allow(irrefutable_let_patterns)]
			if let $pat = actual {
				// ok
			} else {
				assert!(false, "Expected event {:?}. Got {:?}", stringify!($pat), actual);
			}
		)*
	};
}

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
		assert_eq!(Flip::total_balance_of(&ALICE), stake_a1);

		// Add some more
		assert_ok!(StakeManager::staked(Origin::root(), ALICE, stake_a2, ETH_DUMMY_ADDR));
		assert_ok!(StakeManager::staked(Origin::root(), BOB, stake_b, ETH_DUMMY_ADDR));

		// Check storage again.
		assert_eq!(Flip::total_balance_of(&ALICE), stake_a1 + stake_a2);
		assert_eq!(Flip::total_balance_of(&BOB), stake_b);

		// Now claim some FLIP.
		assert_ok!(StakeManager::claim(Origin::signed(ALICE), claim_a, ETH_DUMMY_ADDR));
		assert_ok!(StakeManager::claim(Origin::signed(BOB), claim_b, ETH_DUMMY_ADDR));

		// Make sure it was subtracted.
		assert_eq!(Flip::total_balance_of(&ALICE), stake_a1 + stake_a2 - claim_a);
		assert_eq!(Flip::total_balance_of(&BOB), stake_b - claim_b);

		// Check the pending claims
		assert_eq!(PendingClaims::<Test>::get(ALICE).unwrap().amount, claim_a);
		assert_eq!(PendingClaims::<Test>::get(BOB).unwrap().amount, claim_b);

		assert_event_stack!(
			Event::pallet_cf_staking(crate::Event::ClaimSigRequested(BOB, _, 1, amount)) => {
				assert_eq!(claim_b, amount);
			},
			_, // claim debited from BOB
			Event::pallet_cf_staking(crate::Event::ClaimSigRequested(ALICE, _, 1, amount)) => {
				assert_eq!(claim_a, amount);
			},
			_, // claim debited from ALICE
			Event::pallet_cf_staking(crate::Event::Staked(BOB, staked, total)) => {
				assert_eq!(staked, stake_b);
				assert_eq!(total, stake_b);
			},
			_, // stake credited to BOB
			Event::pallet_cf_staking(crate::Event::Staked(ALICE, staked, total)) => {
				assert_eq!(staked, stake_a2);
				assert_eq!(total, stake_a1 + stake_a2);
			},
			_, // stake credited to ALICE
			Event::pallet_cf_staking(crate::Event::Staked(ALICE, staked, total)) => {
				assert_eq!(staked, stake_a1);
				assert_eq!(total, stake_a1);
			},
			_ // stake credited to ALICE
		);
	});
}

#[test]
fn claiming_unclaimable_is_err() {
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 100_000;

		// Claim FLIP before it is staked.
		assert_noop!(
			StakeManager::claim(Origin::signed(ALICE), STAKE, ETH_DUMMY_ADDR), 
			FlipError::InsufficientLiquidity
		);

		// Make sure account balance hasn't been touched.
		assert_eq!(Flip::total_balance_of(&ALICE), 0u128);

		// Stake some FLIP.
		assert_ok!(StakeManager::staked(Origin::root(), ALICE, STAKE, ETH_DUMMY_ADDR));

		// Claim FLIP from another account.
		assert_noop!(
			StakeManager::claim(Origin::signed(BOB), STAKE, ETH_DUMMY_ADDR), 
			FlipError::InsufficientLiquidity
		);
		
		// Make sure storage hasn't been touched.
		assert_eq!(Flip::total_balance_of(&ALICE), STAKE);

		assert_event_stack!(
			Event::pallet_cf_staking(crate::Event::Staked(ALICE, STAKE, STAKE))
		);
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
		assert_eq!(Flip::total_balance_of(&ALICE), 0u128);
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

		assert_event_stack!(
			Event::pallet_cf_staking(crate::Event::ClaimSettled(ALICE, claimed_amount)) => {
				assert_eq!(claimed_amount, stake);
			},
			Event::pallet_cf_staking(crate::Event::ClaimSigRequested(ALICE, _, nonce, stake)) => {
				assert_eq!(nonce, 1);
			},
			_, // Claim debited from account
			Event::pallet_cf_staking(crate::Event::Staked(ALICE, added, total)) => { 
				assert_eq!(added, stake);
				assert_eq!(total, stake);
			}
		);
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
fn signature_is_inserted() {
	new_test_ext().execute_with(|| {
		let stake = 45u128;
		let sig = Signature::from_slice(&[1u8; 65]);

		// Stake some FLIP.
		assert_ok!(StakeManager::staked(Origin::root(), ALICE, stake, ETH_DUMMY_ADDR));

		// Claim it.
		assert_ok!(StakeManager::claim(Origin::signed(ALICE), stake, ETH_DUMMY_ADDR));

		// Check storage for the signature, should not be there.
		assert_eq!(PendingClaims::<Test>::get(ALICE).unwrap().signature, None);

		// Get the nonce
		let nonce = if let Event::pallet_cf_staking(crate::Event::ClaimSigRequested( _, _, nonce, _ )) =
			frame_system::Pallet::<Test>::events().last().unwrap().event
		{
			nonce
		} else {
			panic!("Expected ClaimSigRequested event with nonce.")
		};
		
		// Insert a signature.
		let expiry = time_after::<Test>(Duration::from_secs(10));
		assert_ok!(StakeManager::post_claim_signature(
			Origin::signed(ALICE),
			ALICE,
			stake,
			nonce,
			ETH_DUMMY_ADDR,
			expiry,
			sig.clone()));

		// Check storage for the signature.
		assert_eq!(PendingClaims::<Test>::get(ALICE).unwrap().signature, Some(sig.clone()));

		assert_event_stack!(
			Event::pallet_cf_staking(crate::Event::ClaimSignatureIssued(ALICE, ..)),
			Event::pallet_cf_staking(crate::Event::ClaimSigRequested(ALICE, _, n, stake)) => {
				assert_eq!(n, nonce);
			},
			_,
			Event::pallet_cf_staking(crate::Event::Staked(ALICE, added, total)) => { 
				assert_eq!(added, stake);
				assert_eq!(total, stake);
			}
		);
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
		assert_eq!(Flip::total_balance_of(&ALICE), 0);

		// Bob votes again (the mock allows this)
		assert_ok!(StakeManager::witness_staked(Origin::signed(BOB), ALICE, 123, ETH_DUMMY_ADDR));

		// Alice should be staked since we set the threshold to 2.
		assert_eq!(Flip::total_balance_of(&ALICE), 123);
	});
}

#[test]
fn cannot_claim_bond() {
	new_test_ext().execute_with(|| {
		let stake = 200u128;
		let bond = 100u128;
		epoch_info::Mock::set_bond(100);
		epoch_info::Mock::add_validator(ALICE);

		// Alice and Bob stake the same amount. 
		assert_ok!(StakeManager::staked(Origin::root(), ALICE, stake, ETH_DUMMY_ADDR));
		assert_ok!(StakeManager::staked(Origin::root(), BOB, stake, ETH_DUMMY_ADDR));

		// Alice becomes a validator
		Flip::set_validator_bond(&ALICE, bond);

		// Bob can withdraw all, but not Alice.
		assert_ok!(StakeManager::claim(Origin::signed(BOB), stake, ETH_DUMMY_ADDR));
		assert_noop!(
			StakeManager::claim(Origin::signed(ALICE), stake, ETH_DUMMY_ADDR),
			FlipError::InsufficientLiquidity
		);

		// Alice *can* withdraw 100
		assert_ok!(StakeManager::claim(Origin::signed(ALICE), 100, ETH_DUMMY_ADDR));

		// Even if she claims, the remaining 100 are blocked
		assert_ok!(StakeManager::claimed(Origin::root(), ALICE, 100));
		assert_noop!(
			StakeManager::claim(Origin::signed(ALICE), 1, ETH_DUMMY_ADDR),
			FlipError::InsufficientLiquidity
		);

		// Once she is no longer bonded, Alice can claim her stake.
		Flip::set_validator_bond(&ALICE, 0u128);
		assert_ok!(StakeManager::claim(Origin::signed(ALICE), 100, ETH_DUMMY_ADDR));
	});
}

#[test]
fn test_retirement() {
	new_test_ext().execute_with(|| {
		epoch_info::Mock::add_validator(ALICE);

		// Need to be staked in order to retire or activate.
		assert_noop!(StakeManager::retire_account(Origin::signed(ALICE)), <Error<Test>>::UnknownAccount);
		assert_noop!(StakeManager::activate_account(Origin::signed(ALICE)), <Error<Test>>::UnknownAccount);

		// Try again with some stake, should succeed this time. 
		assert_ok!(StakeManager::staked(Origin::root(), ALICE, 100, ETH_DUMMY_ADDR));
		assert_ok!(StakeManager::retire_account(Origin::signed(ALICE)));

		assert!(StakeManager::is_retired(&ALICE).unwrap());

		// Can't retire if already retired
		assert_noop!(StakeManager::retire_account(Origin::signed(ALICE)), <Error<Test>>::AlreadyRetired);

		// Reactivate the account
		assert_ok!(StakeManager::activate_account(Origin::signed(ALICE)));

		// Already activated, can't do so again
		assert_noop!(StakeManager::activate_account(Origin::signed(ALICE)), <Error<Test>>::AlreadyActive);

		assert_event_stack!(
			Event::pallet_cf_staking(crate::Event::AccountActivated(_)),
			Event::pallet_cf_staking(crate::Event::AccountRetired(_))
		);
	});
}

#[test]
fn claim_expiry() {
	new_test_ext().execute_with(|| {
		let stake = 45u128;
		let sig = Signature::from_slice(&[1u8; 65]);
		let nonce = 1;

		// Start the time at the 10-second mark.
		time_source::Mock::reset_to(Duration::from_secs(10));

		// Stake some FLIP.
		assert_ok!(StakeManager::staked(Origin::root(), ALICE, stake, ETH_DUMMY_ADDR));
		assert_ok!(StakeManager::staked(Origin::root(), BOB, stake, ETH_DUMMY_ADDR));
		assert_ok!(StakeManager::staked(Origin::root(), CHARLIE, stake, ETH_DUMMY_ADDR));

		// Claim it.
		assert_ok!(StakeManager::claim(Origin::signed(ALICE), stake, ETH_DUMMY_ADDR));
		assert_ok!(StakeManager::claim(Origin::signed(BOB), stake, ETH_DUMMY_ADDR));
		assert_ok!(StakeManager::claim(Origin::signed(CHARLIE), stake, ETH_DUMMY_ADDR));

		// Insert a signature with expiry in the past.
		let expiry = Duration::from_secs(1);
		assert_noop!(
			StakeManager::post_claim_signature(
				Origin::signed(ALICE),
				ALICE,
				stake,
				nonce,
				ETH_DUMMY_ADDR,
				expiry,
				sig.clone()), 
			<Error<Test>>::InvalidExpiry
		);

		// Insert a signature with imminent expiry.
		let expiry = time_after::<Test>(Duration::from_millis(1));
		assert_noop!(
			StakeManager::post_claim_signature(
				Origin::signed(ALICE),
				ALICE,
				stake,
				nonce,
				ETH_DUMMY_ADDR,
				expiry,
				sig.clone()), 
			<Error<Test>>::InvalidExpiry
		);

		// Finally a valid expiry (minimum set to 100ms in the mock).
		let expiry = time_after::<Test>(Duration::from_millis(101));
		assert_ok!(
			StakeManager::post_claim_signature(
				Origin::signed(ALICE),
				ALICE,
				stake,
				nonce,
				ETH_DUMMY_ADDR,
				expiry,
				sig.clone())
		);

		// Set a longer expiry time for Bob.
		let expiry = time_after::<Test>(Duration::from_secs(2));
		assert_ok!(
			StakeManager::post_claim_signature(
				Origin::signed(BOB),
				BOB,
				stake,
				nonce,
				ETH_DUMMY_ADDR,
				expiry,
				sig.clone())
		);

		// Race condition: Charlie's expiry is shorter than Bob's even though his signature is added after.
		let expiry = time_after::<Test>(Duration::from_millis(500));
		assert_ok!(
			StakeManager::post_claim_signature(
				Origin::signed(ALICE),
				CHARLIE,
				stake,
				nonce,
				ETH_DUMMY_ADDR,
				expiry,
				sig.clone())
		);

		Pallet::<Test>::expire_pending_claims();
		
		// Clock hasn't moved, nothing should have expired.
		assert!(PendingClaims::<Test>::contains_key(ALICE));
		assert!(PendingClaims::<Test>::contains_key(BOB));
		assert!(PendingClaims::<Test>::contains_key(CHARLIE));
		
		// Tick the clock forward by 1 sec and expire.
		time_source::Mock::tick(Duration::from_secs(1));
		Pallet::<Test>::expire_pending_claims();

		// It should expire Alice and Charlie's claims but not Bob's.
		assert_event_stack!(
			Event::pallet_cf_flip(FlipEvent::BalanceSettled(
				ImbalanceSource::External, ImbalanceSource::Account(CHARLIE), 45, 0)),
			Event::pallet_cf_staking(crate::Event::ClaimExpired(CHARLIE, _, 45)),
			Event::pallet_cf_flip(FlipEvent::BalanceSettled(
				ImbalanceSource::External, ImbalanceSource::Account(ALICE), 45, 0)),
			Event::pallet_cf_staking(crate::Event::ClaimExpired(ALICE, _, 45))
		);
		assert!(!PendingClaims::<Test>::contains_key(ALICE));
		assert!(PendingClaims::<Test>::contains_key(BOB));
		assert!(!PendingClaims::<Test>::contains_key(CHARLIE));
	});
}

#[test]
fn no_claims_during_auction() {
	new_test_ext().execute_with(|| {
		let stake = 45u128;
		epoch_info::Mock::set_is_auction_phase(true);

		// Staking during an auction is OK.
		assert_ok!(StakeManager::staked(
			Origin::root(),
			ALICE,
			stake,
			ETH_DUMMY_ADDR
		));

		// Claiming during an auction isn't OK.
		assert_noop!(StakeManager::claim(
				Origin::signed(ALICE),
				stake,
				ETH_DUMMY_ADDR
			),
			<Error<Test>>::NoClaimsDuringAuctionPhase
		);
	});
}
