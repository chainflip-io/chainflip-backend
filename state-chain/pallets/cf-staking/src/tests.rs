use crate::{ClaimDetails, ClaimDetailsFor, Config, Error, EthereumAddress, Pallet, PendingClaims, eth_encoding::ClaimRequestPayload, mock::*, pallet};
use std::time::Duration;
use frame_support::{assert_noop, assert_ok, error::BadOrigin, traits::UnixTime};
use sp_core::ecdsa::Signature;
use cf_traits::mocks::epoch_info;
use pallet_cf_flip::ImbalanceSource;

type FlipError = pallet_cf_flip::Error<Test>;
type FlipEvent = pallet_cf_flip::Event<Test>;

const ETH_DUMMY_ADDR: EthereumAddress = [42u8; 20];
const TX_HASH: pallet::EthTransactionHash = [211; 32];

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
				$(
					$test
				)?
			} else {
				assert!(false, "Expected event {:?}. Got {:?}", stringify!($pat), actual);
			}
		)*
	};
}

#[test]
fn staked_amount_is_added_and_subtracted() {
	new_test_ext().execute_with(|| {
		const STAKE_A1: u128 = 45;
		const STAKE_A2: u128 = 21;
		const CLAIM_A: u128 = 44;
		const STAKE_B: u128 = 78;
		const CLAIM_B: u128 = 78;

		// Accounts don't exist yet.
		assert!(!frame_system::Pallet::<Test>::account_exists(&ALICE));
		assert!(!frame_system::Pallet::<Test>::account_exists(&BOB));

		// Dispatch a signed extrinsic to stake some FLIP.
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE_A1, ETH_DUMMY_ADDR, TX_HASH));
		// Read pallet storage and assert the balance was added.
		assert_eq!(Flip::total_balance_of(&ALICE), STAKE_A1);

		// Add some more
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE_A2, ETH_DUMMY_ADDR, TX_HASH));
		assert_ok!(Staking::staked(Origin::root(), BOB, STAKE_B, ETH_DUMMY_ADDR, TX_HASH));

		// Both accounts should now be created. 
		assert!(frame_system::Pallet::<Test>::account_exists(&ALICE));
		assert!(frame_system::Pallet::<Test>::account_exists(&BOB));

		// Check storage again.
		assert_eq!(Flip::total_balance_of(&ALICE), STAKE_A1 + STAKE_A2);
		assert_eq!(Flip::total_balance_of(&BOB), STAKE_B);

		// Now claim some FLIP.
		assert_ok!(Staking::claim(Origin::signed(ALICE), CLAIM_A, ETH_DUMMY_ADDR));
		assert_ok!(Staking::claim(Origin::signed(BOB), CLAIM_B, ETH_DUMMY_ADDR));

		// Make sure it was subtracted.
		assert_eq!(Flip::total_balance_of(&ALICE), STAKE_A1 + STAKE_A2 - CLAIM_A);
		assert_eq!(Flip::total_balance_of(&BOB), STAKE_B - CLAIM_B);

		// Check the pending claims
		assert_eq!(PendingClaims::<Test>::get(ALICE).unwrap().amount, CLAIM_A);
		assert_eq!(PendingClaims::<Test>::get(BOB).unwrap().amount, CLAIM_B);

		assert_event_stack!(
			Event::pallet_cf_staking(crate::Event::ClaimSigRequested(BOB, _payload)),
			_, // claim debited from BOB
			Event::pallet_cf_staking(crate::Event::ClaimSigRequested(ALICE, _payload)),
			_, // claim debited from ALICE
			Event::pallet_cf_staking(crate::Event::Staked(BOB, staked, total)) => {
				assert_eq!(staked, STAKE_B);
				assert_eq!(total, STAKE_B);
			},
			_, // stake credited to BOB
			Event::frame_system(frame_system::Event::NewAccount(BOB)),
			Event::pallet_cf_staking(crate::Event::Staked(ALICE, staked, total)) => {
				assert_eq!(staked, STAKE_A2);
				assert_eq!(total, STAKE_A1 + STAKE_A2);
			},
			_, // stake credited to ALICE
			Event::pallet_cf_staking(crate::Event::Staked(ALICE, staked, total)) => {
				assert_eq!(staked, STAKE_A1);
				assert_eq!(total, STAKE_A1);
			},
			_, // stake credited to ALICE
			Event::frame_system(frame_system::Event::NewAccount(ALICE))
		);
	});
}

#[test]
fn claiming_unclaimable_is_err() {
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 100;

		// Claim FLIP before it is staked.
		assert_noop!(
			Staking::claim(Origin::signed(ALICE), STAKE, ETH_DUMMY_ADDR), 
			FlipError::InsufficientLiquidity
		);

		// Make sure account balance hasn't been touched.
		assert_eq!(Flip::total_balance_of(&ALICE), 0u128);

		// Stake some FLIP.
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE, ETH_DUMMY_ADDR, TX_HASH));

		// Claim FLIP from another account.
		assert_noop!(
			Staking::claim(Origin::signed(BOB), STAKE, ETH_DUMMY_ADDR), 
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
		assert_ok!(Staking::staked(Origin::root(), ALICE, stake_a1 + stake_a2, ETH_DUMMY_ADDR, TX_HASH));

		// Claim a portion.
		assert_ok!(Staking::claim(Origin::signed(ALICE), stake_a1, ETH_DUMMY_ADDR));

		// Claiming the rest should not be possible yet.
		assert_noop!(
			Staking::claim(Origin::signed(ALICE), stake_a2, ETH_DUMMY_ADDR),
			<Error<Test>>::PendingClaim
		);

		// Redeem the first claim.
		assert_ok!(Staking::claimed(Origin::root(), ALICE, stake_a1, 1));

		// Should now be able to claim the rest.
		assert_ok!(Staking::claim(Origin::signed(ALICE), stake_a2, ETH_DUMMY_ADDR));

		// Redeem the rest.
		assert_ok!(Staking::claimed(Origin::root(), ALICE, stake_a2, 2));

		// Remaining stake should be zero
		assert_eq!(Flip::total_balance_of(&ALICE), 0u128);
	});
}

#[test]
fn staked_and_claimed_events_must_match() {
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 45;

		// Account doesn't exist yet.
		assert!(!frame_system::Pallet::<Test>::account_exists(&ALICE));

		// Stake some FLIP.
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE, ETH_DUMMY_ADDR, TX_HASH));

		// The act of staking creates the account.
		assert!(frame_system::Pallet::<Test>::account_exists(&ALICE));

		// Claim it.
		assert_ok!(Staking::claim(Origin::signed(ALICE), STAKE, ETH_DUMMY_ADDR));

		// Invalid Claimed Event from Ethereum: wrong account.
		assert_noop!(Staking::claimed(Origin::root(), BOB, STAKE, 1), <Error<Test>>::NoPendingClaim);

		// Invalid Claimed Event from Ethereum: wrong amount.
		assert_noop!(Staking::claimed(Origin::root(), ALICE, STAKE - 1, 1), <Error<Test>>::InvalidClaimDetails);

		// Invalid Claimed Event from Ethereum: wrong nonce.
		assert_noop!(Staking::claimed(Origin::root(), ALICE, STAKE - 1, 100), <Error<Test>>::InvalidClaimDetails);

		// Valid Claimed Event from Ethereum.
		assert_ok!(Staking::claimed(Origin::root(), ALICE, STAKE, 1));

		// The account balance is now zero, it should have been reaped.
		assert!(!frame_system::Pallet::<Test>::account_exists(&ALICE));

		assert_event_stack!(
			Event::pallet_cf_staking(crate::Event::ClaimSettled(ALICE, claimed_amount)) => {
				assert_eq!(claimed_amount, STAKE);
			},
			Event::frame_system(frame_system::Event::KilledAccount(ALICE)),
			Event::pallet_cf_staking(crate::Event::ClaimSigRequested(ALICE, _payload)),
			_, // Claim debited from account
			Event::pallet_cf_staking(crate::Event::Staked(ALICE, added, total)) => { 
				assert_eq!(added, STAKE);
				assert_eq!(total, STAKE);
			},
			_, // stake credited to ALICE
			Event::frame_system(frame_system::Event::NewAccount(ALICE))
		);
	});
}

#[test]
fn multisig_endpoints_cant_be_called_from_invalid_origins() {
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 45;

		assert_noop!(Staking::staked(Origin::none(), ALICE, STAKE, ETH_DUMMY_ADDR, TX_HASH), BadOrigin);
		assert_noop!(Staking::staked(Origin::signed(Default::default()), ALICE, STAKE, ETH_DUMMY_ADDR, TX_HASH), BadOrigin);

		assert_noop!(Staking::claimed(Origin::none(), ALICE, STAKE, 1), BadOrigin);
		assert_noop!(Staking::claimed(Origin::signed(Default::default()), ALICE, STAKE, 1), BadOrigin);
	});
}

#[test]
fn signature_is_inserted() {
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 45;
		let sig = Signature::from_slice(&[1u8; 65]);

		// Stake some FLIP.
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE, ETH_DUMMY_ADDR, TX_HASH));

		// Claim it.
		assert_ok!(Staking::claim(Origin::signed(ALICE), STAKE, ETH_DUMMY_ADDR));

		// Check storage for the signature, should not be there.
		assert_eq!(PendingClaims::<Test>::get(ALICE).unwrap().signature, None);
		
		// Nonce should be 1.
		let nonce = PendingClaims::<Test>::get(ALICE).unwrap().nonce;
		assert_eq!(nonce, 1);
		
		// Insert a signature.
		let expiry = time_after::<Test>(Duration::from_secs(10));
		assert_ok!(Staking::post_claim_signature(
			Origin::signed(ALICE),
			ALICE,
			STAKE,
			nonce,
			ETH_DUMMY_ADDR,
			expiry,
			sig.clone()));

		// Check storage for the signature.
		assert_eq!(PendingClaims::<Test>::get(ALICE).unwrap().signature, Some(sig.clone()));

		assert_event_stack!(
			Event::pallet_cf_staking(crate::Event::ClaimSignatureIssued(ALICE, ..)),
			Event::pallet_cf_staking(crate::Event::ClaimSigRequested(ALICE, _payload)),
			_,
			Event::pallet_cf_staking(crate::Event::Staked(ALICE, added, total)) => { 
				assert_eq!(added, STAKE);
				assert_eq!(total, STAKE);
			}
		);
	});
}

#[test]
fn witnessing_witnesses() {
	new_test_ext().execute_with(|| {
		witnesser::Mock::set_threshold(2);

		// Bob votes
		assert_ok!(Staking::witness_staked(Origin::signed(BOB), ALICE, 123, ETH_DUMMY_ADDR, TX_HASH));

		// Should be one vote but not staked yet.
		let count = witnesser::Mock::get_vote_count();
		assert_eq!(count, 1);
		assert_eq!(Flip::total_balance_of(&ALICE), 0);

		// Bob votes again (the mock allows this)
		assert_ok!(Staking::witness_staked(Origin::signed(BOB), ALICE, 123, ETH_DUMMY_ADDR, TX_HASH));

		// Alice should be staked since we set the threshold to 2.
		assert_eq!(Flip::total_balance_of(&ALICE), 123);
	});
}

#[test]
fn cannot_claim_bond() {
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 200;
		const BOND: u128 = 102;
		epoch_info::Mock::set_bond(BOND);
		epoch_info::Mock::add_validator(ALICE);

		// Alice and Bob stake the same amount. 
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE, ETH_DUMMY_ADDR, TX_HASH));
		assert_ok!(Staking::staked(Origin::root(), BOB, STAKE, ETH_DUMMY_ADDR, TX_HASH));

		// Alice becomes a validator
		Flip::set_validator_bond(&ALICE, BOND);

		// Bob can withdraw all, but not Alice.
		assert_ok!(Staking::claim(Origin::signed(BOB), STAKE, ETH_DUMMY_ADDR));
		assert_noop!(
			Staking::claim(Origin::signed(ALICE), STAKE, ETH_DUMMY_ADDR),
			FlipError::InsufficientLiquidity
		);

		// Alice *can* withdraw 100
		assert_ok!(Staking::claim(Origin::signed(ALICE), STAKE - BOND, ETH_DUMMY_ADDR));

		// Even if she claims, the remaining 100 are blocked
		assert_ok!(Staking::claimed(Origin::root(), ALICE, STAKE - BOND, 1));
		assert_noop!(
			Staking::claim(Origin::signed(ALICE), 1, ETH_DUMMY_ADDR),
			FlipError::InsufficientLiquidity
		);

		// Once she is no longer bonded, Alice can claim her stake.
		Flip::set_validator_bond(&ALICE, 0u128);
		assert_ok!(Staking::claim(Origin::signed(ALICE), BOND, ETH_DUMMY_ADDR));
	});
}

#[test]
fn test_retirement() {
	new_test_ext().execute_with(|| {
		epoch_info::Mock::add_validator(ALICE);

		// Need to be staked in order to retire or activate.
		assert_noop!(Staking::retire_account(Origin::signed(ALICE)), <Error<Test>>::UnknownAccount);
		assert_noop!(Staking::activate_account(Origin::signed(ALICE)), <Error<Test>>::UnknownAccount);

		// Try again with some stake, should succeed this time. 
		assert_ok!(Staking::staked(Origin::root(), ALICE, 100, ETH_DUMMY_ADDR, TX_HASH));
		assert_ok!(Staking::retire_account(Origin::signed(ALICE)));

		assert!(Staking::is_retired(&ALICE).unwrap());

		// Can't retire if already retired
		assert_noop!(Staking::retire_account(Origin::signed(ALICE)), <Error<Test>>::AlreadyRetired);

		// Reactivate the account
		assert_ok!(Staking::activate_account(Origin::signed(ALICE)));

		// Already activated, can't do so again
		assert_noop!(Staking::activate_account(Origin::signed(ALICE)), <Error<Test>>::AlreadyActive);

		assert_event_stack!(
			Event::pallet_cf_staking(crate::Event::AccountActivated(_)),
			Event::pallet_cf_staking(crate::Event::AccountRetired(_))
		);
	});
}

#[test]
fn claim_expiry() {
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 45;
		let sig = Signature::from_slice(&[1u8; 65]);
		let nonce = 1;

		// Start the time at the 10-second mark.
		time_source::Mock::reset_to(Duration::from_secs(10));

		// Stake some FLIP.
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE, ETH_DUMMY_ADDR, TX_HASH));
		assert_ok!(Staking::staked(Origin::root(), BOB, STAKE, ETH_DUMMY_ADDR, TX_HASH));
		assert_ok!(Staking::staked(Origin::root(), CHARLIE, STAKE, ETH_DUMMY_ADDR, TX_HASH));

		// Claim it.
		assert_ok!(Staking::claim(Origin::signed(ALICE), STAKE, ETH_DUMMY_ADDR));
		assert_ok!(Staking::claim(Origin::signed(BOB), STAKE, ETH_DUMMY_ADDR));
		assert_ok!(Staking::claim(Origin::signed(CHARLIE), STAKE, ETH_DUMMY_ADDR));

		// Insert a signature with expiry in the past.
		let expiry = Duration::from_secs(1);
		assert_noop!(
			Staking::post_claim_signature(
				Origin::signed(ALICE),
				ALICE,
				STAKE,
				nonce,
				ETH_DUMMY_ADDR,
				expiry,
				sig.clone()), 
			<Error<Test>>::InvalidExpiry
		);

		// Insert a signature with imminent expiry.
		let expiry = time_after::<Test>(Duration::from_millis(1));
		assert_noop!(
			Staking::post_claim_signature(
				Origin::signed(ALICE),
				ALICE,
				STAKE,
				nonce,
				ETH_DUMMY_ADDR,
				expiry,
				sig.clone()), 
			<Error<Test>>::InvalidExpiry
		);

		// Finally a valid expiry (minimum set to 100ms in the mock).
		let expiry = time_after::<Test>(Duration::from_millis(101));
		assert_ok!(
			Staking::post_claim_signature(
				Origin::signed(ALICE),
				ALICE,
				STAKE,
				nonce,
				ETH_DUMMY_ADDR,
				expiry,
				sig.clone())
		);

		// Set a longer expiry time for Bob.
		let expiry = time_after::<Test>(Duration::from_secs(2));
		assert_ok!(
			Staking::post_claim_signature(
				Origin::signed(BOB),
				BOB,
				STAKE,
				nonce,
				ETH_DUMMY_ADDR,
				expiry,
				sig.clone())
		);

		// Race condition: Charlie's expiry is shorter than Bob's even though his signature is added after.
		let expiry = time_after::<Test>(Duration::from_millis(500));
		assert_ok!(
			Staking::post_claim_signature(
				Origin::signed(ALICE),
				CHARLIE,
				STAKE,
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
				ImbalanceSource::External, ImbalanceSource::Account(CHARLIE), STAKE, 0)),
			Event::pallet_cf_staking(crate::Event::ClaimExpired(CHARLIE, _, STAKE)),
			Event::pallet_cf_flip(FlipEvent::BalanceSettled(
				ImbalanceSource::External, ImbalanceSource::Account(ALICE), STAKE, 0)),
			Event::pallet_cf_staking(crate::Event::ClaimExpired(ALICE, _, STAKE))
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
		assert_ok!(Staking::staked(
			Origin::root(),
			ALICE,
			stake,
			ETH_DUMMY_ADDR,
			TX_HASH
		));

		// Claiming during an auction isn't OK.
		assert_noop!(Staking::claim(
				Origin::signed(ALICE),
				stake,
				ETH_DUMMY_ADDR
			),
			<Error<Test>>::NoClaimsDuringAuctionPhase
		);
	});
}

#[test]
fn test_claim_all() {
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 100;
		const BOND: u128 = 55;

		// Stake some FLIP.
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE, ETH_DUMMY_ADDR, TX_HASH));

		// Alice becomes a validator.
		Flip::set_validator_bond(&ALICE, BOND);

		// Claim all available funds.
		assert_ok!(Staking::claim_all(Origin::signed(ALICE), ETH_DUMMY_ADDR));

		// We should have a claim for the full staked amount minus the bond.
		assert_event_stack!(
			Event::pallet_cf_staking(crate::Event::ClaimSigRequested(ALICE, _)),
			_, // claim debited from ALICE
			Event::pallet_cf_staking(crate::Event::Staked(ALICE, STAKE, STAKE)),
			_ // stake credited to ALICE
		);
	});
}

#[test]
fn test_claim_payload() {
	let claim_details: ClaimDetailsFor<Test> = ClaimDetails {
		amount: 1234567890u128,
		nonce: 123,
		address: ALICE,
		expiry: Duration::from_secs(10),
		signature: None,
	};
	let runtime_payload = ClaimRequestPayload::<Test>::from((&ALICE, &claim_details));

	// Compare with web3 impl generated from ABI json.
	let web3_payload = todo!();
}
