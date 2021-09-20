use crate::{
	mock::*, pallet, Error, EthereumAddress, FailedStakeAttempts, Pallet, PendingClaims,
	WithdrawalAddresses,
};
use cf_chains::eth::{self, ChainflipContractCall};
use cf_traits::mocks::{epoch_info, time_source};
use codec::Encode;
use frame_support::{assert_noop, assert_ok, error::BadOrigin};
use pallet_cf_flip::{ImbalanceSource, InternalSource};
use pallet_cf_signing::Instance0;
use sp_core::U256;
use std::time::Duration;

type FlipError = pallet_cf_flip::Error<Test>;
type FlipEvent = pallet_cf_flip::Event<Test>;
type SigningEvent = pallet_cf_signing::Event<Test, Instance0>;

const ETH_DUMMY_SIG: eth::SchnorrSignature = eth::SchnorrSignature {
	s: [0xcf; 32],
	k_times_g_addr: [0xcf; 20],
};
const ETH_DUMMY_ADDR: EthereumAddress = [42u8; 20];
const TX_HASH: pallet::EthTransactionHash = [211u8; 32];

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
		assert_ok!(Staking::staked(
			Origin::root(),
			ALICE,
			STAKE_A1,
			None,
			TX_HASH,
		));
		// Read pallet storage and assert the balance was added.
		assert_eq!(Flip::total_balance_of(&ALICE), STAKE_A1);

		// Add some more
		assert_ok!(Staking::staked(
			Origin::root(),
			ALICE,
			STAKE_A2,
			None,
			TX_HASH,
		));
		assert_ok!(Staking::staked(Origin::root(), BOB, STAKE_B, None, TX_HASH));

		// Both accounts should now be created.
		assert!(frame_system::Pallet::<Test>::account_exists(&ALICE));
		assert!(frame_system::Pallet::<Test>::account_exists(&BOB));

		// Check storage again.
		assert_eq!(Flip::total_balance_of(&ALICE), STAKE_A1 + STAKE_A2);
		assert_eq!(Flip::total_balance_of(&BOB), STAKE_B);

		// Now claim some FLIP.
		assert_ok!(Staking::claim(
			Origin::signed(ALICE),
			CLAIM_A,
			ETH_DUMMY_ADDR
		));
		assert_ok!(Staking::claim(Origin::signed(BOB), CLAIM_B, ETH_DUMMY_ADDR));

		// Make sure it was subtracted.
		assert_eq!(
			Flip::total_balance_of(&ALICE),
			STAKE_A1 + STAKE_A2 - CLAIM_A
		);
		assert_eq!(Flip::total_balance_of(&BOB), STAKE_B - CLAIM_B);

		// Check the pending claims
		assert_eq!(
			PendingClaims::<Test>::get(ALICE).unwrap().amount,
			CLAIM_A.into()
		);
		assert_eq!(
			PendingClaims::<Test>::get(BOB).unwrap().amount,
			CLAIM_B.into()
		);

		assert_event_stack!(
			Event::pallet_cf_signing_Instance0(SigningEvent::ThresholdSignatureRequest(..)),
			_, // claim debited from BOB
			Event::pallet_cf_signing_Instance0(SigningEvent::ThresholdSignatureRequest(..)),
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
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE, None, TX_HASH));

		// Claim FLIP from another account.
		assert_noop!(
			Staking::claim(Origin::signed(BOB), STAKE, ETH_DUMMY_ADDR),
			FlipError::InsufficientLiquidity
		);

		// Make sure storage hasn't been touched.
		assert_eq!(Flip::total_balance_of(&ALICE), STAKE);

		assert_event_stack!(Event::pallet_cf_staking(crate::Event::Staked(
			ALICE, STAKE, STAKE
		)));
	});
}

#[test]
fn cannot_double_claim() {
	new_test_ext().execute_with(|| {
		let (stake_a1, stake_a2) = (45u128, 21u128);

		// Stake some FLIP.
		assert_ok!(Staking::staked(
			Origin::root(),
			ALICE,
			stake_a1 + stake_a2,
			None,
			TX_HASH
		));

		// Claim a portion.
		assert_ok!(Staking::claim(
			Origin::signed(ALICE),
			stake_a1,
			ETH_DUMMY_ADDR
		));

		// Claiming the rest should not be possible yet.
		assert_noop!(
			Staking::claim(Origin::signed(ALICE), stake_a2, ETH_DUMMY_ADDR),
			<Error<Test>>::PendingClaim
		);

		// Redeem the first claim.
		assert_ok!(Staking::claimed(Origin::root(), ALICE, stake_a1, TX_HASH));

		// Should now be able to claim the rest.
		assert_ok!(Staking::claim(
			Origin::signed(ALICE),
			stake_a2,
			ETH_DUMMY_ADDR
		));

		// Redeem the rest.
		assert_ok!(Staking::claimed(Origin::root(), ALICE, stake_a2, TX_HASH));

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
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE, None, TX_HASH));

		// The act of staking creates the account.
		assert!(frame_system::Pallet::<Test>::account_exists(&ALICE));

		// Claim it.
		assert_ok!(Staking::claim(Origin::signed(ALICE), STAKE, ETH_DUMMY_ADDR));

		// Invalid Claimed Event from Ethereum: wrong account.
		assert_noop!(
			Staking::claimed(Origin::root(), BOB, STAKE, TX_HASH),
			<Error<Test>>::NoPendingClaim
		);

		// Invalid Claimed Event from Ethereum: wrong amount.
		assert_noop!(
			Staking::claimed(Origin::root(), ALICE, STAKE - 1, TX_HASH),
			<Error<Test>>::InvalidClaimDetails
		);

		// Invalid Claimed Event from Ethereum: wrong nonce.
		assert_noop!(
			Staking::claimed(Origin::root(), ALICE, STAKE - 1, TX_HASH),
			<Error<Test>>::InvalidClaimDetails
		);

		// Valid Claimed Event from Ethereum.
		assert_ok!(Staking::claimed(Origin::root(), ALICE, STAKE, TX_HASH));

		// The account balance is now zero, it should have been reaped.
		assert!(!frame_system::Pallet::<Test>::account_exists(&ALICE));

		assert_event_stack!(
			Event::pallet_cf_staking(crate::Event::ClaimSettled(ALICE, claimed_amount)) => {
				assert_eq!(claimed_amount, STAKE);
			},
			Event::frame_system(frame_system::Event::KilledAccount(ALICE)),
			Event::pallet_cf_signing_Instance0(SigningEvent::ThresholdSignatureRequest(..)),
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

		assert_noop!(
			Staking::staked(Origin::none(), ALICE, STAKE, None, TX_HASH),
			BadOrigin
		);
		assert_noop!(
			Staking::staked(
				Origin::signed(Default::default()),
				ALICE,
				STAKE,
				None,
				TX_HASH,
			),
			BadOrigin
		);

		assert_noop!(
			Staking::claimed(Origin::none(), ALICE, STAKE, TX_HASH),
			BadOrigin
		);
		assert_noop!(
			Staking::claimed(Origin::signed(Default::default()), ALICE, STAKE, TX_HASH),
			BadOrigin
		);
	});
}

#[test]
fn signature_is_inserted() {
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 45;
		const START_TIME: Duration = Duration::from_secs(10);

		// Start the time at the 10-second mark.
		time_source::Mock::reset_to(START_TIME);

		// Stake some FLIP.
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE, None, TX_HASH));

		// Claim it.
		assert_ok!(Staking::claim(Origin::signed(ALICE), STAKE, ETH_DUMMY_ADDR));

		// Check storage for the signature, should not be there.
		assert_eq!(
			PendingClaims::<Test>::get(ALICE).unwrap().is_signed(),
			false
		);

		assert_event_stack!(
			Event::pallet_cf_signing_Instance0(SigningEvent::ThresholdSignatureRequest(id, ..)) => {
				// Insert a signature.
				assert_ok!(Signer::signature_success(
					Origin::root(),
					id,
					ETH_DUMMY_SIG));
			}
		);

		assert_event_stack!(Event::pallet_cf_staking(
			crate::Event::ClaimSignatureIssued(ALICE, _)
		));

		// Check storage for the signature.
		assert!(PendingClaims::<Test>::get(ALICE).unwrap().is_signed());
	});
}

#[test]
fn cannot_claim_bond() {
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 200;
		const BOND: u128 = 102;
		MockEpochInfo::set_bond(BOND);
		MockEpochInfo::add_validator(ALICE);

		// Alice and Bob stake the same amount.
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE, None, TX_HASH));
		assert_ok!(Staking::staked(Origin::root(), BOB, STAKE, None, TX_HASH));

		// Alice becomes a validator
		Flip::set_validator_bond(&ALICE, BOND);

		// Bob can withdraw all, but not Alice.
		assert_ok!(Staking::claim(Origin::signed(BOB), STAKE, ETH_DUMMY_ADDR));
		assert_noop!(
			Staking::claim(Origin::signed(ALICE), STAKE, ETH_DUMMY_ADDR),
			FlipError::InsufficientLiquidity
		);

		// Alice *can* withdraw 100
		assert_ok!(Staking::claim(
			Origin::signed(ALICE),
			STAKE - BOND,
			ETH_DUMMY_ADDR
		));

		// Even if she claims, the remaining 100 are blocked
		assert_ok!(Staking::claimed(
			Origin::root(),
			ALICE,
			STAKE - BOND,
			TX_HASH
		));
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
		MockEpochInfo::add_validator(ALICE);

		// Need to be staked in order to retire or activate.
		assert_noop!(
			Staking::retire_account(Origin::signed(ALICE)),
			<Error<Test>>::UnknownAccount
		);
		assert_noop!(
			Staking::activate_account(Origin::signed(ALICE)),
			<Error<Test>>::UnknownAccount
		);

		// Try again with some stake, should succeed this time.
		assert_ok!(Staking::staked(Origin::root(), ALICE, 100, None, TX_HASH));
		assert_ok!(Staking::retire_account(Origin::signed(ALICE)));

		assert!(Staking::is_retired(&ALICE).unwrap());

		// Can't retire if already retired
		assert_noop!(
			Staking::retire_account(Origin::signed(ALICE)),
			<Error<Test>>::AlreadyRetired
		);

		// Reactivate the account
		assert_ok!(Staking::activate_account(Origin::signed(ALICE)));

		// Already activated, can't do so again
		assert_noop!(
			Staking::activate_account(Origin::signed(ALICE)),
			<Error<Test>>::AlreadyActive
		);

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
		const START_TIME: Duration = Duration::from_secs(10);

		// Start the time at the 10-second mark.
		time_source::Mock::reset_to(START_TIME);

		// Stake some FLIP.
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE, None, TX_HASH));
		assert_ok!(Staking::staked(Origin::root(), BOB, STAKE, None, TX_HASH));

		// Alice claims immediately.
		assert_ok!(Staking::claim(Origin::signed(ALICE), STAKE, ETH_DUMMY_ADDR));

		// Bob claims a little later.
		time_source::Mock::tick(Duration::from_secs(3));
		assert_ok!(Staking::claim(Origin::signed(BOB), STAKE, ETH_DUMMY_ADDR));

		// We can't insert a sig if the claim has expired.
		time_source::Mock::reset_to(START_TIME);
		time_source::Mock::tick(Duration::from_secs(10));
		assert_noop!(
			Staking::post_claim_signature(Origin::root(), ALICE, ETH_DUMMY_SIG),
			<Error<Test>>::SignatureTooLate
		);

		// We can't insert a sig if expiry is too close either.
		time_source::Mock::reset_to(START_TIME);
		time_source::Mock::tick(Duration::from_secs(8));
		assert_noop!(
			Staking::post_claim_signature(Origin::root(), ALICE, ETH_DUMMY_SIG),
			<Error<Test>>::SignatureTooLate
		);

		// If we stay within the defined bounds, we can claim.
		time_source::Mock::reset_to(START_TIME);
		time_source::Mock::tick(Duration::from_secs(4));
		assert_ok!(Staking::post_claim_signature(
			Origin::root(),
			ALICE,
			ETH_DUMMY_SIG
		));

		// Trigger expiry.
		Pallet::<Test>::expire_pending_claims();

		// Nothing should have expired yet.
		assert!(PendingClaims::<Test>::contains_key(ALICE));
		assert!(PendingClaims::<Test>::contains_key(BOB));

		// Tick the clock forward and expire.
		time_source::Mock::tick(Duration::from_secs(7));
		Pallet::<Test>::expire_pending_claims();

		// Alice should have expired but not Bob.
		assert!(!PendingClaims::<Test>::contains_key(ALICE));
		assert!(PendingClaims::<Test>::contains_key(BOB));
		assert_event_stack!(
			Event::pallet_cf_flip(FlipEvent::BalanceSettled(
				ImbalanceSource::External,
				ImbalanceSource::Internal(InternalSource::Account(ALICE)),
				STAKE,
				0
			)),
			Event::pallet_cf_staking(crate::Event::ClaimExpired(ALICE, STAKE))
		);

		// Tick forward again and expire.
		time_source::Mock::tick(Duration::from_secs(10));
		Pallet::<Test>::expire_pending_claims();

		// Bob's (unsigned) claim should now be expired too.
		assert!(!PendingClaims::<Test>::contains_key(BOB));
		assert_event_stack!(
			Event::pallet_cf_flip(FlipEvent::BalanceSettled(
				ImbalanceSource::External,
				ImbalanceSource::Internal(InternalSource::Account(BOB)),
				STAKE,
				0
			)),
			Event::pallet_cf_staking(crate::Event::ClaimExpired(BOB, STAKE))
		);
	});
}

#[test]
fn no_claims_during_auction() {
	new_test_ext().execute_with(|| {
		let stake = 45u128;
		MockEpochInfo::set_is_auction_phase(true);

		// Staking during an auction is OK.
		assert_ok!(Staking::staked(Origin::root(), ALICE, stake, None, TX_HASH));

		// Claiming during an auction isn't OK.
		assert_noop!(
			Staking::claim(Origin::signed(ALICE), stake, ETH_DUMMY_ADDR),
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
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE, None, TX_HASH));

		// Alice becomes a validator.
		Flip::set_validator_bond(&ALICE, BOND);

		// Claim all available funds.
		assert_ok!(Staking::claim_all(Origin::signed(ALICE), ETH_DUMMY_ADDR));

		// We should have a claim for the full staked amount minus the bond.
		assert_event_stack!(
			Event::pallet_cf_signing_Instance0(SigningEvent::ThresholdSignatureRequest(..)),
			_, // claim debited from ALICE
			Event::pallet_cf_staking(crate::Event::Staked(ALICE, STAKE, STAKE)),
			_ // stake credited to ALICE
		);
	});
}

#[test]
fn test_check_withdrawal_address() {
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 45;
		const DIFFERENT_ETH_ADDR: EthereumAddress = [45u8; 20];
		// Case: No account and no address provided
		assert!(Pallet::<Test>::check_withdrawal_address(&ALICE, None, STAKE).is_ok());
		assert!(!WithdrawalAddresses::<Test>::contains_key(ALICE));
		assert!(!FailedStakeAttempts::<Test>::contains_key(ALICE));
		// Case: No account and provided withdrawal address
		assert_ok!(Pallet::<Test>::check_withdrawal_address(
			&ALICE,
			Some(ETH_DUMMY_ADDR),
			STAKE
		));
		let withdrawal_address = WithdrawalAddresses::<Test>::get(ALICE);
		assert!(withdrawal_address.is_some());
		assert_eq!(withdrawal_address.unwrap(), ETH_DUMMY_ADDR);
		// Case: User has already staked with a different address
		Pallet::<Test>::stake_account(&ALICE, STAKE);
		assert!(
			Pallet::<Test>::check_withdrawal_address(&ALICE, Some(DIFFERENT_ETH_ADDR), STAKE)
				.is_err()
		);
		let stake_attempts = FailedStakeAttempts::<Test>::get(ALICE);
		assert_eq!(stake_attempts.len(), 1);
		let stake_attempt = stake_attempts.get(0);
		assert_eq!(stake_attempt.unwrap().0, DIFFERENT_ETH_ADDR);
		assert_eq!(stake_attempt.unwrap().1, STAKE);
		assert_event_stack!(Event::pallet_cf_staking(crate::Event::FailedStakeAttempt(
			..
		)));
		// Case: User stakes again with the same address
		assert!(
			Pallet::<Test>::check_withdrawal_address(&ALICE, Some(ETH_DUMMY_ADDR), STAKE).is_ok()
		);
	});
}

#[test]
fn claim_with_withdrawal_address() {
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 45;
		const WRONG_ETH_ADDR: EthereumAddress = [45u8; 20];
		// Stake some FLIP.
		assert_ok!(Staking::staked(
			Origin::root(),
			ALICE,
			STAKE,
			Some(ETH_DUMMY_ADDR),
			TX_HASH
		));
		// Claim it - expect to fail cause the the address is different
		assert_noop!(
			Staking::claim(Origin::signed(ALICE), STAKE, WRONG_ETH_ADDR),
			<Error<Test>>::WithdrawalAddressRestricted
		);
		// Try it again with the right address - expect to succeed
		assert_ok!(Staking::claim(Origin::signed(ALICE), STAKE, ETH_DUMMY_ADDR));
	});
}

#[test]
fn stake_with_provided_withdrawal_only_on_first_attempt() {
	// Check if the branching of the stake process is working probably
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 45;
		// Stake some FLIP with no withdrawal address
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE, None, TX_HASH));
		// Expect an Staked event to be fired
		assert_event_stack!(Event::pallet_cf_staking(crate::Event::Staked(..)));
		// Stake some FLIP again with an provided withdrawal address
		assert_ok!(Staking::staked(
			Origin::root(),
			ALICE,
			STAKE,
			Some(ETH_DUMMY_ADDR),
			TX_HASH
		));
		// Expect an failed stake event to be fired but no stake event
		assert_event_stack!(Event::pallet_cf_staking(crate::Event::FailedStakeAttempt(
			..
		)));
	});
}
