use crate::{
	mock::*, pallet, AccountRetired, ClaimExpiries, Error, EthereumAddress, FailedStakeAttempts,
	Pallet, PendingClaims, WithdrawalAddresses,
};
use cf_chains::RegisterClaim;
use cf_test_utilities::assert_event_sequence;
use cf_traits::{
	mocks::{system_state_info::MockSystemStateInfo, time_source},
	Bonding,
};

use frame_support::{assert_noop, assert_ok, error::BadOrigin};
use pallet_cf_flip::{Bonder, ImbalanceSource, InternalSource};
use sp_runtime::DispatchError;
use std::time::Duration;

type FlipError = pallet_cf_flip::Error<Test>;
type FlipEvent = pallet_cf_flip::Event<Test>;

const ETH_DUMMY_ADDR: EthereumAddress = [42u8; 20];
const ETH_ZERO_ADDRESS: EthereumAddress = [0xff; 20];
const TX_HASH: pallet::EthTransactionHash = [211u8; 32];

#[test]
fn genesis_nodes_are_activated_by_default() {
	new_test_ext().execute_with(|| {
		// Expect the genesis node to be activated.
		assert!(AccountRetired::<Test>::contains_key(&CHARLIE));
		// Expect a not genesis node not to be activated.
		assert!(!AccountRetired::<Test>::contains_key(&ALICE));
	});
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
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE_A1, ETH_ZERO_ADDRESS, TX_HASH,));
		// Read pallet storage and assert the balance was added.
		assert_eq!(Flip::total_balance_of(&ALICE), STAKE_A1);

		// Add some more
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE_A2, ETH_ZERO_ADDRESS, TX_HASH,));
		assert_ok!(Staking::staked(Origin::root(), BOB, STAKE_B, ETH_ZERO_ADDRESS, TX_HASH));

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
		assert_eq!(PendingClaims::<Test>::get(ALICE).unwrap().amount(), CLAIM_A);
		assert_eq!(PendingClaims::<Test>::get(BOB).unwrap().amount(), CLAIM_B);

		// Two threshold signature requests should have been made.
		assert_eq!(MockThresholdSigner::received_requests().len(), 2);

		assert_event_sequence!(
			Test,
			Event::System(frame_system::Event::NewAccount(ALICE)),
			Event::Flip(pallet_cf_flip::Event::BalanceSettled(
				ImbalanceSource::External,
				ImbalanceSource::Internal(InternalSource::Account(ALICE)),
				STAKE_A1,
				0
			)),
			Event::Staking(crate::Event::Staked(ALICE, STAKE_A1, STAKE_A1)),
			Event::Flip(pallet_cf_flip::Event::BalanceSettled(
				ImbalanceSource::External,
				ImbalanceSource::Internal(InternalSource::Account(ALICE)),
				STAKE_A2,
				0
			)),
			Event::Staking(crate::Event::Staked(ALICE, STAKE_A2, STAKE_A1 + STAKE_A2)),
			Event::System(frame_system::Event::NewAccount(BOB)),
			Event::Flip(pallet_cf_flip::Event::BalanceSettled(
				ImbalanceSource::External,
				ImbalanceSource::Internal(InternalSource::Account(BOB)),
				STAKE_B,
				0
			)),
			Event::Staking(crate::Event::Staked(BOB, STAKE_B, STAKE_B)),
			Event::Flip(pallet_cf_flip::Event::BalanceSettled(
				ImbalanceSource::Internal(InternalSource::Account(ALICE)),
				ImbalanceSource::External,
				CLAIM_A,
				0
			)),
			Event::Flip(pallet_cf_flip::Event::BalanceSettled(
				ImbalanceSource::Internal(InternalSource::Account(BOB)),
				ImbalanceSource::External,
				STAKE_B,
				0
			))
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
			Error::<Test>::InvalidClaim
		);

		// Make sure account balance hasn't been touched.
		assert_eq!(Flip::total_balance_of(&ALICE), 0u128);

		// Stake some FLIP.
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE, ETH_ZERO_ADDRESS, TX_HASH));

		// Try to, and fail, claim an amount that would leave the balance below the minimum stake
		let excessive_claim = STAKE - MIN_STAKE + 1;
		assert_noop!(
			Staking::claim(Origin::signed(ALICE), excessive_claim, ETH_DUMMY_ADDR),
			Error::<Test>::BelowMinimumStake
		);

		// Claim FLIP from another account.
		assert_noop!(
			Staking::claim(Origin::signed(BOB), STAKE, ETH_DUMMY_ADDR),
			Error::<Test>::InvalidClaim
		);

		// Make sure storage hasn't been touched.
		assert_eq!(Flip::total_balance_of(&ALICE), STAKE);

		assert_event_sequence!(
			Test,
			Event::System(frame_system::Event::NewAccount(ALICE)),
			Event::Flip(pallet_cf_flip::Event::BalanceSettled(
				ImbalanceSource::External,
				ImbalanceSource::Internal(InternalSource::Account(ALICE)),
				STAKE,
				0
			)),
			Event::Staking(crate::Event::Staked(ALICE, STAKE, STAKE))
		);
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
			ETH_ZERO_ADDRESS,
			TX_HASH
		));

		// Claim a portion.
		assert_ok!(Staking::claim(Origin::signed(ALICE), stake_a1, ETH_DUMMY_ADDR));

		// Claiming the rest should not be possible yet.
		assert_noop!(
			Staking::claim(Origin::signed(ALICE), stake_a2, ETH_DUMMY_ADDR),
			<Error<Test>>::PendingClaim
		);

		// Redeem the first claim.
		assert_eq!(
			ClaimExpiries::<Test>::get()[0].1,
			ALICE,
			"Alice's claim should have an expiry set"
		);
		assert_ok!(Staking::claimed(Origin::root(), ALICE, stake_a1, TX_HASH));
		assert_eq!(
			ClaimExpiries::<Test>::get().len(),
			0,
			"As Alice's claim is claimed it should have no expiry"
		);

		// Should now be able to claim the rest.
		assert_ok!(Staking::claim(Origin::signed(ALICE), stake_a2, ETH_DUMMY_ADDR));

		// Redeem the rest.
		assert_eq!(
			ClaimExpiries::<Test>::get()[0].1,
			ALICE,
			"Alice's claim should have an expiry set"
		);
		assert_ok!(Staking::claimed(Origin::root(), ALICE, stake_a2, TX_HASH));
		assert_eq!(
			ClaimExpiries::<Test>::get().len(),
			0,
			"As Alice's claim is claimed it should have no expiry"
		);

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
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE, ETH_ZERO_ADDRESS, TX_HASH));

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

		// Threshold signature request should have been made.
		assert_eq!(MockThresholdSigner::received_requests().len(), 1);

		assert_event_sequence!(
			Test,
			Event::System(frame_system::Event::NewAccount(ALICE)),
			Event::Flip(pallet_cf_flip::Event::BalanceSettled(
				ImbalanceSource::External,
				ImbalanceSource::Internal(InternalSource::Account(ALICE)),
				STAKE,
				0
			)),
			Event::Staking(crate::Event::Staked(ALICE, STAKE, STAKE)),
			Event::Flip(pallet_cf_flip::Event::BalanceSettled(
				ImbalanceSource::Internal(InternalSource::Account(ALICE)),
				ImbalanceSource::External,
				STAKE,
				0
			)),
			Event::System(frame_system::Event::KilledAccount(ALICE)),
			Event::Staking(crate::Event::ClaimSettled(ALICE, STAKE))
		);
	});
}

#[test]
fn multisig_endpoints_cant_be_called_from_invalid_origins() {
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 45;

		assert_noop!(
			Staking::staked(Origin::none(), ALICE, STAKE, ETH_ZERO_ADDRESS, TX_HASH),
			BadOrigin
		);
		assert_noop!(
			Staking::staked(
				Origin::signed(Default::default()),
				ALICE,
				STAKE,
				ETH_ZERO_ADDRESS,
				TX_HASH,
			),
			BadOrigin
		);

		assert_noop!(Staking::claimed(Origin::none(), ALICE, STAKE, TX_HASH), BadOrigin);
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
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE, ETH_ZERO_ADDRESS, TX_HASH));

		// Claim it.
		assert_ok!(Staking::claim(Origin::signed(ALICE), STAKE, ETH_DUMMY_ADDR));

		// Threshold signature request should have been made.
		assert_eq!(MockThresholdSigner::received_requests().len(), 1);

		// Threshold signature generated.
		MockThresholdSigner::on_signature_ready(&ALICE).unwrap();

		assert_event_sequence!(
			Test,
			Event::System(frame_system::Event::NewAccount(ALICE)),
			Event::Flip(pallet_cf_flip::Event::BalanceSettled(
				ImbalanceSource::External,
				ImbalanceSource::Internal(InternalSource::Account(ALICE)),
				STAKE,
				0
			)),
			Event::Staking(crate::Event::Staked(ALICE, STAKE, STAKE)),
			Event::Flip(pallet_cf_flip::Event::BalanceSettled(
				ImbalanceSource::Internal(InternalSource::Account(ALICE)),
				ImbalanceSource::External,
				STAKE,
				0
			)),
			Event::Staking(crate::Event::ClaimSignatureIssued(
				ALICE,
				vec![
					26, 207, 82, 35, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 207, 207, 207, 207, 207,
					207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 0,
					0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
					0, 0, 0, 122, 105, 249, 102, 238, 241, 89, 232, 39, 185, 33, 125, 210, 208,
					147, 185, 206, 123, 93, 154, 198, 139, 192, 212, 144, 47, 233, 178, 176, 182,
					4, 171, 175, 231, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207,
					207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207,
					207, 207, 207, 207, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
					0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 207,
					207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207,
					207, 207, 207, 161, 161, 161, 161, 161, 161, 161, 161, 161, 161, 161, 161, 161,
					161, 161, 161, 161, 161, 161, 161, 161, 161, 161, 161, 161, 161, 161, 161, 161,
					161, 161, 161, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
					0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 45, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 42, 42,
					42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 0, 0,
					0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
					0, 0, 0, 20
				]
			))
		);

		// Check storage for the signature.
		assert!(PendingClaims::<Test>::contains_key(ALICE));
		let api_call = frame_support::storage::unhashed::get::<cf_chains::eth::api::EthereumApi>(
			PendingClaims::<Test>::hashed_key_for(ALICE).as_slice(),
		)
		.expect("there should be a pending claim at this point");

		let claim = match api_call {
			cf_chains::eth::api::EthereumApi::RegisterClaim(inner) => inner,
			_ => panic!("Wrong api call."),
		};

		assert_eq!(claim.sig_data.get_signature(), ETH_DUMMY_SIG);
	});
}

#[test]
fn cannot_claim_bond() {
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 200;
		const BOND: u128 = 102;
		MockEpochInfo::set_bond(BOND);
		MockEpochInfo::add_authorities(ALICE);

		// Alice and Bob stake the same amount.
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE, ETH_ZERO_ADDRESS, TX_HASH));
		assert_ok!(Staking::staked(Origin::root(), BOB, STAKE, ETH_ZERO_ADDRESS, TX_HASH));

		// Alice becomes an authority
		Bonder::<Test>::update_bond(&ALICE, BOND);

		// Bob can withdraw all, but not Alice.
		assert_ok!(Staking::claim(Origin::signed(BOB), STAKE, ETH_DUMMY_ADDR));
		assert_noop!(
			Staking::claim(Origin::signed(ALICE), STAKE, ETH_DUMMY_ADDR),
			FlipError::InsufficientLiquidity
		);

		// Alice *can* withdraw 100
		assert_ok!(Staking::claim(Origin::signed(ALICE), STAKE - BOND, ETH_DUMMY_ADDR));

		// Even if she claims, the remaining 100 are blocked
		assert_ok!(Staking::claimed(Origin::root(), ALICE, STAKE - BOND, TX_HASH));
		assert_noop!(
			Staking::claim(Origin::signed(ALICE), 1, ETH_DUMMY_ADDR),
			FlipError::InsufficientLiquidity
		);

		// Once she is no longer bonded, Alice can claim her stake.
		Bonder::<Test>::update_bond(&ALICE, 0u128);
		assert_ok!(Staking::claim(Origin::signed(ALICE), BOND, ETH_DUMMY_ADDR));
	});
}

#[test]
fn test_retirement() {
	new_test_ext().execute_with(|| {
		MockEpochInfo::add_authorities(ALICE);
		const STAKE: u128 = 100;

		// Need to be staked in order to retire or activate.
		assert_noop!(Staking::retire_account(Origin::signed(ALICE)), <Error<Test>>::UnknownAccount);
		assert_noop!(
			Staking::activate_account(Origin::signed(ALICE)),
			<Error<Test>>::UnknownAccount
		);

		// Try again with some stake, should succeed this time.
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE, ETH_ZERO_ADDRESS, TX_HASH));

		// Expect the account to be retired by default
		assert!(Staking::is_retired(&ALICE).unwrap());

		// Can't retire if retired
		assert_noop!(Staking::retire_account(Origin::signed(ALICE)), <Error<Test>>::AlreadyRetired);

		// Activate the account
		assert_ok!(Staking::activate_account(Origin::signed(ALICE)));

		// Already activated, can't do so again
		assert_noop!(
			Staking::activate_account(Origin::signed(ALICE)),
			<Error<Test>>::AlreadyActive
		);

		assert_event_sequence!(
			Test,
			Event::System(frame_system::Event::NewAccount(ALICE)),
			Event::Flip(pallet_cf_flip::Event::BalanceSettled(
				ImbalanceSource::External,
				ImbalanceSource::Internal(InternalSource::Account(ALICE)),
				STAKE,
				0
			)),
			Event::Staking(crate::Event::Staked(ALICE, STAKE, STAKE)),
			Event::Staking(crate::Event::AccountActivated(ALICE))
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
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE, ETH_ZERO_ADDRESS, TX_HASH));
		assert_ok!(Staking::staked(Origin::root(), BOB, STAKE, ETH_ZERO_ADDRESS, TX_HASH));

		// Alice claims immediately.
		assert_ok!(Staking::claim(Origin::signed(ALICE), STAKE, ETH_DUMMY_ADDR));

		// Bob claims a little later.
		time_source::Mock::tick(Duration::from_secs(3));
		assert_ok!(Staking::claim(Origin::signed(BOB), STAKE, ETH_DUMMY_ADDR));

		// If we stay within the defined bounds, we can claim.
		time_source::Mock::reset_to(START_TIME);
		time_source::Mock::tick(Duration::from_secs(4));
		assert_ok!(Staking::post_claim_signature(Origin::root(), ALICE, 0));

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

		// Tick forward again and expire.
		time_source::Mock::tick(Duration::from_secs(10));
		Pallet::<Test>::expire_pending_claims();

		// Bob's (unsigned) claim should now be expired too.
		assert!(!PendingClaims::<Test>::contains_key(BOB));

		assert_event_sequence!(
			Test,
			Event::System(frame_system::Event::NewAccount(ALICE)),
			Event::Flip(FlipEvent::BalanceSettled(
				ImbalanceSource::External,
				ImbalanceSource::Internal(InternalSource::Account(ALICE)),
				STAKE,
				0
			)),
			Event::Staking(crate::Event::Staked(ALICE, STAKE, STAKE)),
			Event::System(frame_system::Event::NewAccount(BOB)),
			Event::Flip(FlipEvent::BalanceSettled(
				ImbalanceSource::External,
				ImbalanceSource::Internal(InternalSource::Account(BOB)),
				STAKE,
				0
			)),
			Event::Staking(crate::Event::Staked(BOB, STAKE, STAKE)),
			Event::Flip(FlipEvent::BalanceSettled(
				ImbalanceSource::Internal(InternalSource::Account(ALICE)),
				ImbalanceSource::External,
				STAKE,
				0
			)),
			Event::Flip(FlipEvent::BalanceSettled(
				ImbalanceSource::Internal(InternalSource::Account(BOB)),
				ImbalanceSource::External,
				STAKE,
				0
			)),
			Event::Staking(crate::Event::ClaimSignatureIssued(
				ALICE,
				vec![
					26, 207, 82, 35, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 207, 207, 207, 207, 207,
					207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 0,
					0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
					0, 0, 0, 122, 105, 249, 102, 238, 241, 89, 232, 39, 185, 33, 125, 210, 208,
					147, 185, 206, 123, 93, 154, 198, 139, 192, 212, 144, 47, 233, 178, 176, 182,
					4, 171, 175, 231, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207,
					207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207,
					207, 207, 207, 207, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
					0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 207,
					207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207, 207,
					207, 207, 207, 161, 161, 161, 161, 161, 161, 161, 161, 161, 161, 161, 161, 161,
					161, 161, 161, 161, 161, 161, 161, 161, 161, 161, 161, 161, 161, 161, 161, 161,
					161, 161, 161, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
					0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 45, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 42, 42,
					42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 0, 0,
					0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
					0, 0, 0, 20
				]
			)),
			Event::Staking(crate::Event::ClaimExpired(ALICE, STAKE)),
			Event::Flip(FlipEvent::BalanceSettled(
				ImbalanceSource::External,
				ImbalanceSource::Internal(InternalSource::Account(ALICE)),
				STAKE,
				0
			)),
			Event::Staking(crate::Event::ClaimExpired(BOB, STAKE)),
			Event::Flip(FlipEvent::BalanceSettled(
				ImbalanceSource::External,
				ImbalanceSource::Internal(InternalSource::Account(BOB)),
				STAKE,
				0
			))
		);
	});
}

#[test]
fn no_claims_allowed_out_of_claim_period() {
	new_test_ext().execute_with(|| {
		let stake = 45u128;
		MockEpochInfo::set_is_auction_phase(true);

		// Staking during an auction is OK.
		assert_ok!(Staking::staked(Origin::root(), ALICE, stake, ETH_ZERO_ADDRESS, TX_HASH));

		// Claiming is not allowed.
		assert_noop!(
			Staking::claim(Origin::signed(ALICE), stake, ETH_DUMMY_ADDR),
			<Error<Test>>::AuctionPhase
		);
	});
}

#[test]
fn test_claim_all() {
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 100;
		const BOND: u128 = 55;

		// Stake some FLIP.
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE, ETH_ZERO_ADDRESS, TX_HASH));

		// Alice becomes an authority.
		Bonder::<Test>::update_bond(&ALICE, BOND);

		// Claim all available funds.
		assert_ok!(Staking::claim_all(Origin::signed(ALICE), ETH_DUMMY_ADDR));

		// We should have a claim for the full staked amount minus the bond.
		assert_event_sequence!(
			Test,
			Event::System(frame_system::Event::NewAccount(ALICE)),
			Event::Flip(pallet_cf_flip::Event::BalanceSettled(
				ImbalanceSource::External,
				ImbalanceSource::Internal(InternalSource::Account(ALICE)),
				100,
				0
			)),
			Event::Staking(crate::Event::Staked(ALICE, STAKE, STAKE)),
			Event::Flip(pallet_cf_flip::Event::BalanceSettled(
				ImbalanceSource::Internal(InternalSource::Account(ALICE)),
				ImbalanceSource::External,
				STAKE - BOND,
				0
			))
		);
	});
}

#[test]
fn test_check_withdrawal_address() {
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 45;
		const DIFFERENT_ETH_ADDR: EthereumAddress = [45u8; 20];
		// Case: No account and no address provided
		assert!(Pallet::<Test>::check_withdrawal_address(&ALICE, ETH_ZERO_ADDRESS, STAKE).is_ok());
		assert!(!WithdrawalAddresses::<Test>::contains_key(ALICE));
		assert!(!FailedStakeAttempts::<Test>::contains_key(ALICE));
		// Case: No account and provided withdrawal address
		assert_ok!(Pallet::<Test>::check_withdrawal_address(&ALICE, ETH_DUMMY_ADDR, STAKE));
		let withdrawal_address = WithdrawalAddresses::<Test>::get(ALICE);
		assert!(withdrawal_address.is_some());
		assert_eq!(withdrawal_address.unwrap(), ETH_DUMMY_ADDR);
		// Case: User has already staked with a different address
		Pallet::<Test>::stake_account(&ALICE, STAKE);
		assert!(
			Pallet::<Test>::check_withdrawal_address(&ALICE, DIFFERENT_ETH_ADDR, STAKE).is_err()
		);
		let stake_attempts = FailedStakeAttempts::<Test>::get(ALICE);
		assert_eq!(stake_attempts.len(), 1);
		let stake_attempt = stake_attempts.get(0);
		assert_eq!(stake_attempt.unwrap().0, DIFFERENT_ETH_ADDR);
		assert_eq!(stake_attempt.unwrap().1, STAKE);
		for e in System::events().into_iter().map(|e| e.event) {
			println!("{:?}", e);
		}
		assert_event_sequence!(
			Test,
			Event::System(frame_system::Event::NewAccount(ALICE)),
			Event::Flip(pallet_cf_flip::Event::BalanceSettled(
				ImbalanceSource::External,
				ImbalanceSource::Internal(InternalSource::Account(ALICE)),
				STAKE,
				0
			)),
			Event::Staking(crate::Event::Staked(ALICE, STAKE, STAKE)),
			Event::Staking(crate::Event::FailedStakeAttempt(ALICE, DIFFERENT_ETH_ADDR, STAKE))
		);
		// Case: User stakes again with the same address
		assert!(Pallet::<Test>::check_withdrawal_address(&ALICE, ETH_DUMMY_ADDR, STAKE).is_ok());
	});
}

#[test]
fn claim_with_withdrawal_address() {
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 45;
		const WRONG_ETH_ADDR: EthereumAddress = [45u8; 20];
		// Stake some FLIP.
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE, ETH_DUMMY_ADDR, TX_HASH));
		// Claim it - expect to fail because the address is different
		assert_noop!(
			Staking::claim(Origin::signed(ALICE), STAKE, WRONG_ETH_ADDR),
			<Error<Test>>::WithdrawalAddressRestricted
		);
		// Try it again with the right address - expect to succeed
		assert_ok!(Staking::claim(Origin::signed(ALICE), STAKE, ETH_DUMMY_ADDR));
	});
}

#[test]
fn cannot_claim_to_zero_address() {
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 45;
		const ETH_ZERO_ADDRESS: EthereumAddress = [0xff; 20];
		// Stake some FLIP, we use the zero address here to denote that we should be
		// able to claim to any address in future
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE, ETH_ZERO_ADDRESS, TX_HASH));
		// Claim it - expect to fail because the address is the zero address
		assert_noop!(
			Staking::claim(Origin::signed(ALICE), STAKE, ETH_ZERO_ADDRESS),
			<Error<Test>>::InvalidClaim
		);
		// Try it again with a non-zero address - expect to succeed
		assert_ok!(Staking::claim(Origin::signed(ALICE), STAKE, ETH_DUMMY_ADDR));
	});
}

#[test]
fn stake_with_provided_withdrawal_only_on_first_attempt() {
	// Check if the branching of the stake process is working probably
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 45;
		// Stake some FLIP with no withdrawal address
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE, ETH_ZERO_ADDRESS, TX_HASH));
		// Stake some FLIP again with an provided withdrawal address
		assert_ok!(Staking::staked(Origin::root(), ALICE, STAKE, ETH_DUMMY_ADDR, TX_HASH));
		// Expect an failed stake event to be fired but no stake event
		assert_event_sequence!(
			Test,
			Event::System(frame_system::Event::NewAccount(ALICE)),
			Event::Flip(pallet_cf_flip::Event::BalanceSettled(
				ImbalanceSource::External,
				ImbalanceSource::Internal(InternalSource::Account(ALICE)),
				STAKE,
				0
			)),
			Event::Staking(crate::Event::Staked(ALICE, STAKE, STAKE)),
			Event::Staking(crate::Event::FailedStakeAttempt(ALICE, ETH_DUMMY_ADDR, STAKE))
		);
	});
}

#[test]
fn maintenance_mode() {
	new_test_ext().execute_with(|| {
		MockSystemStateInfo::set_maintenance(true);
		assert_noop!(
			Staking::staked(Origin::root(), ALICE, 20, ETH_DUMMY_ADDR, TX_HASH),
			DispatchError::Other("We are in maintenance!")
		);
		assert_noop!(
			Staking::claimed(Origin::root(), ALICE, 20, TX_HASH),
			DispatchError::Other("We are in maintenance!")
		);
		MockSystemStateInfo::set_maintenance(false);
	});
}
