use crate::{
	mock::*, pallet, ActiveBidder, ClaimAmount, ClaimExpiries, Error, EthereumAddress,
	FailedStakeAttempts, Pallet, PendingClaims, WithdrawalAddresses,
};
use cf_test_utilities::assert_event_sequence;
use cf_traits::{
	mocks::{system_state_info::MockSystemStateInfo, time_source},
	Bonding,
};

use frame_support::{assert_noop, assert_ok, error::BadOrigin, traits::Hooks};
use pallet_cf_flip::Bonder;
use sp_runtime::DispatchError;
use std::time::Duration;

type FlipError = pallet_cf_flip::Error<Test>;

const ETH_DUMMY_ADDR: EthereumAddress = [42u8; 20];
const ETH_ZERO_ADDRESS: EthereumAddress = [0xff; 20];
const TX_HASH: pallet::EthTransactionHash = [211u8; 32];

#[test]
fn genesis_nodes_are_activated_by_default() {
	new_test_ext().execute_with(|| {
		// Expect the genesis node to be activated.
		assert!(ActiveBidder::<Test>::contains_key(&CHARLIE));
		// Expect a not genesis node not to be activated.
		assert!(!ActiveBidder::<Test>::contains_key(&ALICE));
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
		assert_ok!(Staking::staked(
			RuntimeOrigin::root(),
			ALICE,
			STAKE_A1,
			ETH_ZERO_ADDRESS,
			TX_HASH,
		));
		// Read pallet storage and assert the balance was added.
		assert_eq!(Flip::total_balance_of(&ALICE), STAKE_A1);

		// Add some more
		assert_ok!(Staking::staked(
			RuntimeOrigin::root(),
			ALICE,
			STAKE_A2,
			ETH_ZERO_ADDRESS,
			TX_HASH,
		));
		assert_ok!(Staking::staked(RuntimeOrigin::root(), BOB, STAKE_B, ETH_ZERO_ADDRESS, TX_HASH));

		// Both accounts should now be created.
		assert!(frame_system::Pallet::<Test>::account_exists(&ALICE));
		assert!(frame_system::Pallet::<Test>::account_exists(&BOB));

		// Check storage again.
		assert_eq!(Flip::total_balance_of(&ALICE), STAKE_A1 + STAKE_A2);
		assert_eq!(Flip::total_balance_of(&BOB), STAKE_B);

		// Now claim some FLIP.
		assert_ok!(Staking::claim(RuntimeOrigin::signed(ALICE), CLAIM_A.into(), ETH_DUMMY_ADDR));
		assert_ok!(Staking::claim(RuntimeOrigin::signed(BOB), CLAIM_B.into(), ETH_DUMMY_ADDR));

		// Make sure it was subtracted.
		assert_eq!(Flip::total_balance_of(&ALICE), STAKE_A1 + STAKE_A2 - CLAIM_A);
		assert_eq!(Flip::total_balance_of(&BOB), STAKE_B - CLAIM_B);

		// Check the pending claims
		assert!(PendingClaims::<Test>::get(ALICE).is_some());
		assert!(PendingClaims::<Test>::get(BOB).is_some());

		// Two broadcasts should have been initiated by the two claims.
		assert_eq!(MockBroadcaster::received_requests().len(), 2);

		assert_event_sequence!(
			Test,
			RuntimeEvent::System(frame_system::Event::NewAccount { account: ALICE }),
			RuntimeEvent::Staking(crate::Event::Staked {
				account_id: ALICE,
				tx_hash: TX_HASH,
				stake_added: STAKE_A1,
				total_stake: STAKE_A1
			}),
			RuntimeEvent::Staking(crate::Event::Staked {
				account_id: ALICE,
				tx_hash: TX_HASH,
				stake_added: STAKE_A2,
				total_stake: STAKE_A1 + STAKE_A2
			}),
			RuntimeEvent::System(frame_system::Event::NewAccount { account: BOB }),
			RuntimeEvent::Staking(crate::Event::Staked {
				account_id: BOB,
				tx_hash: TX_HASH,
				stake_added: STAKE_B,
				total_stake: STAKE_B
			})
		);
	});
}

#[test]
fn claiming_unclaimable_is_err() {
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 100;

		// Claim FLIP before it is staked.
		assert_noop!(
			Staking::claim(RuntimeOrigin::signed(ALICE), STAKE.into(), ETH_DUMMY_ADDR),
			Error::<Test>::InvalidClaim
		);

		// Make sure account balance hasn't been touched.
		assert_eq!(Flip::total_balance_of(&ALICE), 0u128);

		// Stake some FLIP.
		assert_ok!(Staking::staked(RuntimeOrigin::root(), ALICE, STAKE, ETH_ZERO_ADDRESS, TX_HASH));

		// Try to, and fail, claim an amount that would leave the balance below the minimum stake
		let excessive_claim = STAKE - MIN_STAKE + 1;
		assert_noop!(
			Staking::claim(RuntimeOrigin::signed(ALICE), excessive_claim.into(), ETH_DUMMY_ADDR),
			Error::<Test>::BelowMinimumStake
		);

		// Claim FLIP from another account.
		assert_noop!(
			Staking::claim(RuntimeOrigin::signed(BOB), STAKE.into(), ETH_DUMMY_ADDR),
			Error::<Test>::InvalidClaim
		);

		// Make sure storage hasn't been touched.
		assert_eq!(Flip::total_balance_of(&ALICE), STAKE);

		assert_event_sequence!(
			Test,
			RuntimeEvent::System(frame_system::Event::NewAccount { account: ALICE }),
			RuntimeEvent::Staking(crate::Event::Staked {
				account_id: ALICE,
				tx_hash: TX_HASH,
				stake_added: STAKE,
				total_stake: STAKE
			})
		);
	});
}

#[test]
fn cannot_double_claim() {
	new_test_ext().execute_with(|| {
		let (stake_a1, stake_a2) = (45u128, 21u128);

		// Stake some FLIP.
		assert_ok!(Staking::staked(
			RuntimeOrigin::root(),
			ALICE,
			stake_a1 + stake_a2,
			ETH_ZERO_ADDRESS,
			TX_HASH
		));

		// Claim a portion.
		assert_ok!(Staking::claim(RuntimeOrigin::signed(ALICE), stake_a1.into(), ETH_DUMMY_ADDR));

		// Claiming the rest should not be possible yet.
		assert_noop!(
			Staking::claim(RuntimeOrigin::signed(ALICE), stake_a1.into(), ETH_DUMMY_ADDR),
			<Error<Test>>::PendingClaim
		);

		assert_eq!(
			ClaimExpiries::<Test>::get()[0].1,
			ALICE,
			"Alice's claim should have an expiry set"
		);
		assert_ok!(Staking::claimed(RuntimeOrigin::root(), ALICE, stake_a1, TX_HASH));
		assert_eq!(
			ClaimExpiries::<Test>::get().len(),
			0,
			"As Alice's claim is claimed it should have no expiry"
		);
		assert!(PendingClaims::<Test>::get(&ALICE).is_none());

		// Should now be able to claim the rest.
		assert_ok!(Staking::claim(RuntimeOrigin::signed(ALICE), stake_a2.into(), ETH_DUMMY_ADDR));

		assert_eq!(
			ClaimExpiries::<Test>::get()[0].1,
			ALICE,
			"Alice's claim should have an expiry set"
		);
		assert_ok!(Staking::claimed(RuntimeOrigin::root(), ALICE, stake_a2, TX_HASH));
		assert_eq!(
			ClaimExpiries::<Test>::get().len(),
			0,
			"As Alice's claim is claimed it should have no expiry"
		);
		assert!(PendingClaims::<Test>::get(&ALICE).is_none());

		// Remaining stake should be zero
		assert_eq!(Flip::total_balance_of(&ALICE), 0u128);
	});
}

#[test]
fn claim_cannot_occur_without_staking_first() {
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 45;

		// Account doesn't exist yet.
		assert!(!frame_system::Pallet::<Test>::account_exists(&ALICE));

		// Stake some FLIP.
		assert_ok!(Staking::staked(RuntimeOrigin::root(), ALICE, STAKE, ETH_ZERO_ADDRESS, TX_HASH));

		// The act of staking creates the account.
		assert!(frame_system::Pallet::<Test>::account_exists(&ALICE));

		// Claim it.
		assert_ok!(Staking::claim(RuntimeOrigin::signed(ALICE), STAKE.into(), ETH_DUMMY_ADDR));

		// Claim should kick off a broadcast request.
		assert_eq!(MockBroadcaster::received_requests().len(), 1);

		// Invalid Claimed Event from Ethereum: wrong account.
		assert_noop!(
			Staking::claimed(RuntimeOrigin::root(), BOB, STAKE, TX_HASH),
			<Error<Test>>::NoPendingClaim
		);

		// Valid Claimed Event from Ethereum.
		assert_ok!(Staking::claimed(RuntimeOrigin::root(), ALICE, STAKE, TX_HASH));

		// The account balance is now zero, it should have been reaped.
		assert!(!frame_system::Pallet::<Test>::account_exists(&ALICE));

		assert_event_sequence!(
			Test,
			RuntimeEvent::System(frame_system::Event::NewAccount { account: ALICE }),
			RuntimeEvent::Staking(crate::Event::Staked {
				account_id: ALICE,
				tx_hash: TX_HASH,
				stake_added: STAKE,
				total_stake: STAKE
			}),
			RuntimeEvent::Staking(crate::Event::ClaimRequested {
				account_id: ALICE,
				amount: STAKE,
				broadcast_id: 0,
				expiry_time: 10,
			}),
			RuntimeEvent::System(frame_system::Event::KilledAccount { account: ALICE }),
			RuntimeEvent::Staking(crate::Event::ClaimSettled(ALICE, STAKE))
		);
	});
}

#[test]
fn multisig_endpoints_cant_be_called_from_invalid_origins() {
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 45;

		assert_noop!(
			Staking::staked(RuntimeOrigin::none(), ALICE, STAKE, ETH_ZERO_ADDRESS, TX_HASH),
			BadOrigin
		);
		assert_noop!(
			Staking::staked(RuntimeOrigin::signed(ALICE), ALICE, STAKE, ETH_ZERO_ADDRESS, TX_HASH,),
			BadOrigin
		);

		assert_noop!(Staking::claimed(RuntimeOrigin::none(), ALICE, STAKE, TX_HASH), BadOrigin);
		assert_noop!(
			Staking::claimed(RuntimeOrigin::signed(ALICE), ALICE, STAKE, TX_HASH),
			BadOrigin
		);
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
		assert_ok!(Staking::staked(RuntimeOrigin::root(), ALICE, STAKE, ETH_ZERO_ADDRESS, TX_HASH));
		assert_ok!(Staking::staked(RuntimeOrigin::root(), BOB, STAKE, ETH_ZERO_ADDRESS, TX_HASH));

		// Alice becomes an authority
		Bonder::<Test>::update_bond(&ALICE, BOND);

		// Bob can withdraw all, but not Alice.
		assert_ok!(Staking::claim(RuntimeOrigin::signed(BOB), STAKE.into(), ETH_DUMMY_ADDR));
		assert_noop!(
			Staking::claim(RuntimeOrigin::signed(ALICE), STAKE.into(), ETH_DUMMY_ADDR),
			FlipError::InsufficientLiquidity
		);

		// Alice *can* withdraw 100
		assert_ok!(Staking::claim(
			RuntimeOrigin::signed(ALICE),
			(STAKE - BOND).into(),
			ETH_DUMMY_ADDR
		));

		// Even if she claims, the remaining 100 are blocked
		assert_ok!(Staking::claimed(RuntimeOrigin::root(), ALICE, STAKE - BOND, TX_HASH));
		assert_noop!(
			Staking::claim(RuntimeOrigin::signed(ALICE), 1.into(), ETH_DUMMY_ADDR),
			FlipError::InsufficientLiquidity
		);

		// Once she is no longer bonded, Alice can claim her stake.
		Bonder::<Test>::update_bond(&ALICE, 0u128);
		assert_ok!(Staking::claim(RuntimeOrigin::signed(ALICE), BOND.into(), ETH_DUMMY_ADDR));
	});
}

#[test]
fn test_retirement() {
	new_test_ext().execute_with(|| {
		MockEpochInfo::add_authorities(ALICE);
		const STAKE: u128 = 100;

		// Need to be staked in order to retire or activate.
		assert_noop!(
			Staking::retire_account(RuntimeOrigin::signed(ALICE)),
			<Error<Test>>::UnknownAccount
		);
		assert_noop!(
			Staking::activate_account(RuntimeOrigin::signed(ALICE)),
			<Error<Test>>::UnknownAccount
		);

		// Try again with some stake, should succeed this time.
		assert_ok!(Staking::staked(RuntimeOrigin::root(), ALICE, STAKE, ETH_ZERO_ADDRESS, TX_HASH));

		// Expect the account to be retired by default
		assert!(!ActiveBidder::<Test>::try_get(ALICE).expect("staking adds bidder status"));

		// Can't retire if retired
		assert_noop!(
			Staking::retire_account(RuntimeOrigin::signed(ALICE)),
			<Error<Test>>::AlreadyRetired
		);

		// Activate the account
		assert_ok!(Staking::activate_account(RuntimeOrigin::signed(ALICE)));
		assert!(ActiveBidder::<Test>::get(ALICE));

		// Already activated, can't do so again
		assert_noop!(
			Staking::activate_account(RuntimeOrigin::signed(ALICE)),
			<Error<Test>>::AlreadyActive
		);

		// Cannot retire if participating in in the auction phase
		MockEpochInfo::set_is_auction_phase(true);
		assert_noop!(
			Staking::retire_account(RuntimeOrigin::signed(ALICE)),
			<Error<Test>>::AuctionPhase
		);
		assert!(ActiveBidder::<Test>::get(ALICE));

		// Can retire outside of auction phase
		MockEpochInfo::set_is_auction_phase(false);
		assert_ok!(Staking::retire_account(RuntimeOrigin::signed(ALICE)));
		assert!(!ActiveBidder::<Test>::get(ALICE));

		assert_event_sequence!(
			Test,
			RuntimeEvent::System(frame_system::Event::NewAccount { account: ALICE }),
			RuntimeEvent::Staking(crate::Event::Staked {
				account_id: ALICE,
				tx_hash: TX_HASH,
				stake_added: STAKE,
				total_stake: STAKE
			}),
			RuntimeEvent::Staking(crate::Event::AccountActivated(ALICE)),
			RuntimeEvent::Staking(crate::Event::AccountRetired(ALICE))
		);
	});
}

#[test]
fn claim_expiry() {
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 45;
		const START_TIME_SECS: u64 = 10;

		// Start the time at the 10-second mark.
		time_source::Mock::reset_to(Duration::from_secs(START_TIME_SECS));

		// Stake some FLIP.
		assert_ok!(Staking::staked(RuntimeOrigin::root(), ALICE, STAKE, ETH_ZERO_ADDRESS, TX_HASH));
		assert_ok!(Staking::staked(RuntimeOrigin::root(), BOB, STAKE, ETH_ZERO_ADDRESS, TX_HASH));

		// Alice claims immediately.
		assert_ok!(Staking::claim(RuntimeOrigin::signed(ALICE), STAKE.into(), ETH_DUMMY_ADDR));

		// Bob claims a little later.
		const BOB_DELAY_SECS: u64 = 3;
		time_source::Mock::tick(Duration::from_secs(BOB_DELAY_SECS));
		assert_ok!(Staking::claim(RuntimeOrigin::signed(BOB), STAKE.into(), ETH_DUMMY_ADDR));

		// If we stay within the defined bounds, we can claim.
		time_source::Mock::reset_to(Duration::from_secs(START_TIME_SECS));
		const INIT_TICK: u64 = 4;
		time_source::Mock::tick(Duration::from_secs(INIT_TICK));

		// Trigger expiry.
		Pallet::<Test>::on_initialize(0);

		// Nothing should have expired yet.
		assert!(PendingClaims::<Test>::contains_key(ALICE));
		assert!(PendingClaims::<Test>::contains_key(BOB));

		// Tick the clock forward and expire.
		const NEXT_TICK: u64 = 7;
		time_source::Mock::tick(Duration::from_secs(NEXT_TICK));
		let total_ticked = NEXT_TICK + INIT_TICK;
		assert!(total_ticked > CLAIM_TTL_SECS);

		Pallet::<Test>::on_initialize(0);

		// Both claims should still exist. We have only passed the *contract* expiry.
		// We still need to wait for `ClaimDelayBufferSeconds` until the claim
		// is expired on the SC.

		assert!(PendingClaims::<Test>::contains_key(ALICE));
		assert!(PendingClaims::<Test>::contains_key(BOB));

		// Now we let `ClaimDelayBufferSeconds` elapse
		time_source::Mock::tick(Duration::from_secs(CLAIM_DELAY_BUFFER_SECS));
		Pallet::<Test>::on_initialize(0);

		// Alice should have expired but not Bob.
		assert!(!PendingClaims::<Test>::contains_key(ALICE));
		assert!(PendingClaims::<Test>::contains_key(BOB));

		// Tick forward again and expire.
		time_source::Mock::tick(Duration::from_secs(10));
		Pallet::<Test>::on_initialize(0);

		// Bob's (unsigned) claim should now be expired too.
		assert!(!PendingClaims::<Test>::contains_key(BOB));

		assert_event_sequence!(
			Test,
			RuntimeEvent::System(frame_system::Event::NewAccount { account: ALICE }),
			RuntimeEvent::Staking(crate::Event::Staked {
				account_id: ALICE,
				tx_hash: TX_HASH,
				stake_added: STAKE,
				total_stake: STAKE
			}),
			RuntimeEvent::System(frame_system::Event::NewAccount { account: BOB }),
			RuntimeEvent::Staking(crate::Event::Staked {
				account_id: BOB,
				tx_hash: TX_HASH,
				stake_added: STAKE,
				total_stake: STAKE
			}),
			RuntimeEvent::Staking(crate::Event::ClaimRequested {
				account_id: ALICE,
				amount: STAKE,
				broadcast_id: 0,
				expiry_time: START_TIME_SECS + CLAIM_TTL_SECS,
			}),
			RuntimeEvent::Staking(crate::Event::ClaimRequested {
				account_id: BOB,
				amount: STAKE,
				broadcast_id: 0,
				expiry_time: START_TIME_SECS + BOB_DELAY_SECS + CLAIM_TTL_SECS,
			}),
			RuntimeEvent::Staking(crate::Event::ClaimExpired { account_id: ALICE }),
			RuntimeEvent::Staking(crate::Event::ClaimExpired { account_id: BOB })
		);
	});
}

#[test]
fn can_only_claim_during_auction_if_retired() {
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 45;
		MockEpochInfo::set_is_auction_phase(true);

		// Stake and activate Alice
		assert_ok!(Staking::staked(RuntimeOrigin::root(), ALICE, STAKE, ETH_ZERO_ADDRESS, TX_HASH));
		assert_ok!(Staking::activate_account(RuntimeOrigin::signed(ALICE)));
		assert!(ActiveBidder::<Test>::get(ALICE));

		// Claiming is not allowed because Alice is bidding in the auction phase.
		assert_noop!(
			Staking::claim(RuntimeOrigin::signed(ALICE), STAKE.into(), ETH_DUMMY_ADDR),
			<Error<Test>>::AuctionPhase
		);

		// Retire Alice (must be done outside of the auction phase)
		MockEpochInfo::set_is_auction_phase(false);
		assert_ok!(Staking::retire_account(RuntimeOrigin::signed(ALICE)));
		assert!(!ActiveBidder::<Test>::get(ALICE));

		// Alice should be able to claim while in the auction phase because she is not bidding
		MockEpochInfo::set_is_auction_phase(true);
		assert_ok!(Staking::claim(RuntimeOrigin::signed(ALICE), STAKE.into(), ETH_DUMMY_ADDR),);
	});
}

#[test]
fn test_claim_all() {
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 100;
		const BOND: u128 = 55;

		// Stake some FLIP.
		assert_ok!(Staking::staked(RuntimeOrigin::root(), ALICE, STAKE, ETH_ZERO_ADDRESS, TX_HASH));

		// Alice becomes an authority.
		Bonder::<Test>::update_bond(&ALICE, BOND);

		// Claim all available funds.
		assert_ok!(Staking::claim(RuntimeOrigin::signed(ALICE), ClaimAmount::Max, ETH_DUMMY_ADDR));
		assert_eq!(Flip::total_balance_of(&ALICE), BOND);

		// We should have a claim for the full staked amount minus the bond.
		assert_event_sequence!(
			Test,
			RuntimeEvent::System(frame_system::Event::NewAccount { account: ALICE }),
			RuntimeEvent::Staking(crate::Event::Staked {
				account_id: ALICE,
				tx_hash: TX_HASH,
				stake_added: STAKE,
				total_stake: STAKE
			})
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
		let stake_attempt = stake_attempts.first();
		assert_eq!(stake_attempt.unwrap().0, DIFFERENT_ETH_ADDR);
		assert_eq!(stake_attempt.unwrap().1, STAKE);
		for e in System::events().into_iter().map(|e| e.event) {
			println!("{e:?}");
		}
		assert_event_sequence!(
			Test,
			RuntimeEvent::System(frame_system::Event::NewAccount { account: ALICE }),
			RuntimeEvent::Staking(crate::Event::FailedStakeAttempt(
				ALICE,
				DIFFERENT_ETH_ADDR,
				STAKE
			))
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
		assert_ok!(Staking::staked(RuntimeOrigin::root(), ALICE, STAKE, ETH_DUMMY_ADDR, TX_HASH));
		// Claim it - expect to fail because the address is different
		assert_noop!(
			Staking::claim(RuntimeOrigin::signed(ALICE), STAKE.into(), WRONG_ETH_ADDR),
			<Error<Test>>::WithdrawalAddressRestricted
		);
		// Try it again with the right address - expect to succeed
		assert_ok!(Staking::claim(RuntimeOrigin::signed(ALICE), STAKE.into(), ETH_DUMMY_ADDR));
	});
}

#[test]
fn cannot_claim_to_zero_address() {
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 45;
		const ETH_ZERO_ADDRESS: EthereumAddress = [0xff; 20];
		// Stake some FLIP, we use the zero address here to denote that we should be
		// able to claim to any address in future
		assert_ok!(Staking::staked(RuntimeOrigin::root(), ALICE, STAKE, ETH_ZERO_ADDRESS, TX_HASH));
		// Claim it - expect to fail because the address is the zero address
		assert_noop!(
			Staking::claim(RuntimeOrigin::signed(ALICE), STAKE.into(), ETH_ZERO_ADDRESS),
			<Error<Test>>::InvalidClaim
		);
		// Try it again with a non-zero address - expect to succeed
		assert_ok!(Staking::claim(RuntimeOrigin::signed(ALICE), STAKE.into(), ETH_DUMMY_ADDR));
	});
}

#[test]
fn stake_with_provided_withdrawal_only_on_first_attempt() {
	// Check if the branching of the stake process is working probably
	new_test_ext().execute_with(|| {
		const STAKE: u128 = 45;
		// Stake some FLIP with no withdrawal address
		assert_ok!(Staking::staked(RuntimeOrigin::root(), ALICE, STAKE, ETH_ZERO_ADDRESS, TX_HASH));
		// Stake some FLIP again with an provided withdrawal address
		assert_ok!(Staking::staked(RuntimeOrigin::root(), ALICE, STAKE, ETH_DUMMY_ADDR, TX_HASH));
		// Expect an failed stake event to be fired but no stake event
		assert_event_sequence!(
			Test,
			RuntimeEvent::System(frame_system::Event::NewAccount { account: ALICE }),
			RuntimeEvent::Staking(crate::Event::Staked {
				account_id: ALICE,
				tx_hash: TX_HASH,
				stake_added: STAKE,
				total_stake: STAKE
			}),
			RuntimeEvent::Staking(crate::Event::FailedStakeAttempt(ALICE, ETH_DUMMY_ADDR, STAKE))
		);
	});
}

#[test]
fn maintenance_mode_blocks_claim_requests() {
	new_test_ext().execute_with(|| {
		MockSystemStateInfo::set_maintenance(true);
		assert_noop!(
			Staking::claim(RuntimeOrigin::signed(ALICE), ClaimAmount::Max, ETH_DUMMY_ADDR),
			DispatchError::Other("We are in maintenance!")
		);
		MockSystemStateInfo::set_maintenance(false);
	});
}
