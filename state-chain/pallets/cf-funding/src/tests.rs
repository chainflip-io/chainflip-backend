use crate::{
	mock::*, pallet, ActiveBidder, Error, EthereumAddress, FailedFundingAttempts, Pallet,
	PendingRedemptions, RedemptionAmount, WithdrawalAddresses,
};
use cf_test_utilities::assert_event_sequence;
use cf_traits::{mocks::system_state_info::MockSystemStateInfo, Bonding};

use frame_support::{assert_noop, assert_ok};
use pallet_cf_flip::Bonder;
use sp_runtime::DispatchError;

type FlipError = pallet_cf_flip::Error<Test>;

const ETH_DUMMY_ADDR: EthereumAddress = [42u8; 20];
const ETH_ZERO_ADDRESS: EthereumAddress = [0xff; 20];
const TX_HASH: pallet::EthTransactionHash = [211u8; 32];

#[test]
fn genesis_nodes_are_bidding_by_default() {
	new_test_ext().execute_with(|| {
		assert!(ActiveBidder::<Test>::contains_key(&CHARLIE));
		assert!(!ActiveBidder::<Test>::contains_key(&ALICE));
	});
}

#[test]
fn funded_amount_is_added_and_subtracted() {
	new_test_ext().execute_with(|| {
		const AMOUNT_A1: u128 = 45;
		const AMOUNT_A2: u128 = 21;
		const REDEMPTION_A: u128 = 44;
		const AMOUNT_B: u128 = 78;
		const REDEMPTION_B: u128 = 78;

		// Accounts don't exist yet.
		assert!(!frame_system::Pallet::<Test>::account_exists(&ALICE));
		assert!(!frame_system::Pallet::<Test>::account_exists(&BOB));

		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			AMOUNT_A1,
			ETH_ZERO_ADDRESS,
			TX_HASH,
		));
		// Read pallet storage and assert the balance was added.
		assert_eq!(Flip::total_balance_of(&ALICE), AMOUNT_A1);

		// Add some more
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			AMOUNT_A2,
			ETH_ZERO_ADDRESS,
			TX_HASH,
		));
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			BOB,
			AMOUNT_B,
			ETH_ZERO_ADDRESS,
			TX_HASH
		));

		// Both accounts should now be created.
		assert!(frame_system::Pallet::<Test>::account_exists(&ALICE));
		assert!(frame_system::Pallet::<Test>::account_exists(&BOB));

		// Check storage again.
		assert_eq!(Flip::total_balance_of(&ALICE), AMOUNT_A1 + AMOUNT_A2);
		assert_eq!(Flip::total_balance_of(&BOB), AMOUNT_B);

		// Now redeem some FLIP.
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			REDEMPTION_A.into(),
			ETH_DUMMY_ADDR
		));
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(BOB),
			REDEMPTION_B.into(),
			ETH_DUMMY_ADDR
		));

		// Make sure it was subtracted.
		assert_eq!(Flip::total_balance_of(&ALICE), AMOUNT_A1 + AMOUNT_A2 - REDEMPTION_A);
		assert_eq!(Flip::total_balance_of(&BOB), AMOUNT_B - REDEMPTION_B);

		// Check the pending redemptions
		assert!(PendingRedemptions::<Test>::get(ALICE).is_some());
		assert!(PendingRedemptions::<Test>::get(BOB).is_some());

		// Two broadcasts should have been initiated by the two redemptions.
		assert_eq!(MockBroadcaster::received_requests().len(), 2);

		assert_event_sequence!(
			Test,
			RuntimeEvent::System(frame_system::Event::NewAccount { account: ALICE }),
			RuntimeEvent::Funding(crate::Event::Funded {
				account_id: ALICE,
				tx_hash: TX_HASH,
				funds_added: AMOUNT_A1,
				total_balance: AMOUNT_A1
			}),
			RuntimeEvent::Funding(crate::Event::Funded {
				account_id: ALICE,
				tx_hash: TX_HASH,
				funds_added: AMOUNT_A2,
				total_balance: AMOUNT_A1 + AMOUNT_A2
			}),
			RuntimeEvent::System(frame_system::Event::NewAccount { account: BOB }),
			RuntimeEvent::Funding(crate::Event::Funded {
				account_id: BOB,
				tx_hash: TX_HASH,
				funds_added: AMOUNT_B,
				total_balance: AMOUNT_B
			})
		);
	});
}

#[test]
fn redeeming_unredeemable_is_err() {
	new_test_ext().execute_with(|| {
		const AMOUNT: u128 = 100;

		// Redeem FLIP before it is funded.
		assert_noop!(
			Funding::redeem(RuntimeOrigin::signed(ALICE), AMOUNT.into(), ETH_DUMMY_ADDR),
			Error::<Test>::InvalidRedemption
		);

		// Make sure account balance hasn't been touched.
		assert_eq!(Flip::total_balance_of(&ALICE), 0u128);

		// Add some funds.
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			AMOUNT,
			ETH_ZERO_ADDRESS,
			TX_HASH
		));

		// Try to, and fail, redeem an amount that would leave the balance below the minimum.
		let excessive_redemption = AMOUNT - MIN_FUNDING + 1;
		assert_noop!(
			Funding::redeem(
				RuntimeOrigin::signed(ALICE),
				excessive_redemption.into(),
				ETH_DUMMY_ADDR
			),
			Error::<Test>::BelowMinimumFunding
		);

		// Redeem FLIP from another account.
		assert_noop!(
			Funding::redeem(RuntimeOrigin::signed(BOB), AMOUNT.into(), ETH_DUMMY_ADDR),
			Error::<Test>::InvalidRedemption
		);

		// Make sure storage hasn't been touched.
		assert_eq!(Flip::total_balance_of(&ALICE), AMOUNT);

		assert_event_sequence!(
			Test,
			RuntimeEvent::System(frame_system::Event::NewAccount { account: ALICE }),
			RuntimeEvent::Funding(crate::Event::Funded {
				account_id: ALICE,
				tx_hash: TX_HASH,
				funds_added: AMOUNT,
				total_balance: AMOUNT
			})
		);
	});
}

#[test]
fn cannot_double_redeem() {
	new_test_ext().execute_with(|| {
		let (amount_a1, amount_a2) = (45u128, 21u128);

		// Add some funds.
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			amount_a1 + amount_a2,
			ETH_ZERO_ADDRESS,
			TX_HASH
		));

		// Redeem a portion.
		assert_ok!(Funding::redeem(RuntimeOrigin::signed(ALICE), amount_a1.into(), ETH_DUMMY_ADDR));

		// Redeeming the rest should not be possible yet.
		assert_noop!(
			Funding::redeem(RuntimeOrigin::signed(ALICE), amount_a1.into(), ETH_DUMMY_ADDR),
			<Error<Test>>::PendingRedemption
		);

		assert_ok!(Funding::redeemed(RuntimeOrigin::root(), ALICE, amount_a1, TX_HASH));
		assert!(PendingRedemptions::<Test>::get(&ALICE).is_none());

		// Should now be able to redeem the rest.
		assert_ok!(Funding::redeem(RuntimeOrigin::signed(ALICE), amount_a2.into(), ETH_DUMMY_ADDR));

		assert_ok!(Funding::redeemed(RuntimeOrigin::root(), ALICE, amount_a2, TX_HASH));
		assert!(PendingRedemptions::<Test>::get(&ALICE).is_none());

		// Remaining amount should be zero
		assert_eq!(Flip::total_balance_of(&ALICE), 0u128);
	});
}

#[test]
fn redemption_cannot_occur_without_funding_first() {
	new_test_ext().execute_with(|| {
		const AMOUNT: u128 = 45;

		// Account doesn't exist yet.
		assert!(!frame_system::Pallet::<Test>::account_exists(&ALICE));

		// Add some funds.
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			AMOUNT,
			ETH_ZERO_ADDRESS,
			TX_HASH
		));

		// The act of funding creates the account.
		assert!(frame_system::Pallet::<Test>::account_exists(&ALICE));

		// Redeem it.
		assert_ok!(Funding::redeem(RuntimeOrigin::signed(ALICE), AMOUNT.into(), ETH_DUMMY_ADDR));

		// Redeem should kick off a broadcast request.
		assert_eq!(MockBroadcaster::received_requests().len(), 1);

		// Invalid Redeemed Event from Ethereum: wrong account.
		assert_noop!(
			Funding::redeemed(RuntimeOrigin::root(), BOB, AMOUNT, TX_HASH),
			<Error<Test>>::NoPendingRedemption
		);

		// Valid Redeemed Event from Ethereum.
		assert_ok!(Funding::redeemed(RuntimeOrigin::root(), ALICE, AMOUNT, TX_HASH));

		// The account balance is now zero, it should have been reaped.
		assert!(!frame_system::Pallet::<Test>::account_exists(&ALICE));

		assert_event_sequence!(
			Test,
			RuntimeEvent::System(frame_system::Event::NewAccount { account: ALICE }),
			RuntimeEvent::Funding(crate::Event::Funded {
				account_id: ALICE,
				tx_hash: TX_HASH,
				funds_added: AMOUNT,
				total_balance: AMOUNT
			}),
			RuntimeEvent::Funding(crate::Event::RedemptionRequested {
				account_id: ALICE,
				amount: AMOUNT,
				broadcast_id: 0,
				expiry_time: 10,
			}),
			RuntimeEvent::System(frame_system::Event::KilledAccount { account: ALICE }),
			RuntimeEvent::Funding(crate::Event::RedemptionSettled(ALICE, AMOUNT))
		);
	});
}

#[test]
fn cannot_redeem_bond() {
	new_test_ext().execute_with(|| {
		const AMOUNT: u128 = 200;
		const BOND: u128 = 102;
		MockEpochInfo::set_bond(BOND);
		MockEpochInfo::add_authorities(ALICE);

		// Alice and Bob fund the same amount.
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			AMOUNT,
			ETH_ZERO_ADDRESS,
			TX_HASH
		));
		assert_ok!(Funding::funded(RuntimeOrigin::root(), BOB, AMOUNT, ETH_ZERO_ADDRESS, TX_HASH));

		// Alice becomes an authority
		Bonder::<Test>::update_bond(&ALICE, BOND);

		// Bob can withdraw all, but not Alice.
		assert_ok!(Funding::redeem(RuntimeOrigin::signed(BOB), AMOUNT.into(), ETH_DUMMY_ADDR));
		assert_noop!(
			Funding::redeem(RuntimeOrigin::signed(ALICE), AMOUNT.into(), ETH_DUMMY_ADDR),
			FlipError::InsufficientLiquidity
		);

		// Alice *can* withdraw 100
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			(AMOUNT - BOND).into(),
			ETH_DUMMY_ADDR
		));

		// Even if she redeems, the remaining 100 are blocked
		assert_ok!(Funding::redeemed(RuntimeOrigin::root(), ALICE, AMOUNT - BOND, TX_HASH));
		assert_noop!(
			Funding::redeem(RuntimeOrigin::signed(ALICE), 1.into(), ETH_DUMMY_ADDR),
			FlipError::InsufficientLiquidity
		);

		// Once she is no longer bonded, Alice can redeem her funds.
		Bonder::<Test>::update_bond(&ALICE, 0u128);
		assert_ok!(Funding::redeem(RuntimeOrigin::signed(ALICE), BOND.into(), ETH_DUMMY_ADDR));
	});
}

#[test]
fn test_stop_bidding() {
	new_test_ext().execute_with(|| {
		MockEpochInfo::add_authorities(ALICE);
		const AMOUNT: u128 = 100;

		// Need to be funded in order to stop or start bidding.
		assert_noop!(
			Funding::stop_bidding(RuntimeOrigin::signed(ALICE)),
			<Error<Test>>::UnknownAccount
		);
		assert_noop!(
			Funding::start_bidding(RuntimeOrigin::signed(ALICE)),
			<Error<Test>>::UnknownAccount
		);

		// Try again with some funds, should succeed this time.
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			AMOUNT,
			ETH_ZERO_ADDRESS,
			TX_HASH
		));

		assert!(!ActiveBidder::<Test>::try_get(ALICE).expect("funding adds bidder status"));

		assert_noop!(
			Funding::stop_bidding(RuntimeOrigin::signed(ALICE)),
			<Error<Test>>::AlreadyNotBidding
		);

		assert_ok!(Funding::start_bidding(RuntimeOrigin::signed(ALICE)));
		assert!(ActiveBidder::<Test>::get(ALICE));

		assert_noop!(
			Funding::start_bidding(RuntimeOrigin::signed(ALICE)),
			<Error<Test>>::AlreadyBidding
		);

		MockEpochInfo::set_is_auction_phase(true);
		assert_noop!(
			Funding::stop_bidding(RuntimeOrigin::signed(ALICE)),
			<Error<Test>>::AuctionPhase
		);
		assert!(ActiveBidder::<Test>::get(ALICE));

		// Can stop bidding if outside of auction phase
		MockEpochInfo::set_is_auction_phase(false);
		assert_ok!(Funding::stop_bidding(RuntimeOrigin::signed(ALICE)));
		assert!(!ActiveBidder::<Test>::get(ALICE));

		assert_event_sequence!(
			Test,
			RuntimeEvent::System(frame_system::Event::NewAccount { account: ALICE }),
			RuntimeEvent::Funding(crate::Event::Funded {
				account_id: ALICE,
				tx_hash: TX_HASH,
				funds_added: AMOUNT,
				total_balance: AMOUNT
			}),
			RuntimeEvent::Funding(crate::Event::StartedBidding { account_id: ALICE }),
			RuntimeEvent::Funding(crate::Event::StoppedBidding { account_id: ALICE })
		);
	});
}

#[test]
fn can_only_redeem_during_auction_if_not_bidding() {
	new_test_ext().execute_with(|| {
		const AMOUNT: u128 = 45;
		MockEpochInfo::set_is_auction_phase(true);

		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			AMOUNT,
			ETH_ZERO_ADDRESS,
			TX_HASH
		));
		assert_ok!(Funding::start_bidding(RuntimeOrigin::signed(ALICE)));
		assert!(ActiveBidder::<Test>::get(ALICE));

		// Redeeming is not allowed because Alice is bidding in the auction phase.
		assert_noop!(
			Funding::redeem(RuntimeOrigin::signed(ALICE), AMOUNT.into(), ETH_DUMMY_ADDR),
			<Error<Test>>::AuctionPhase
		);

		// Stop bidding for Alice (must be done outside of the auction phase)
		MockEpochInfo::set_is_auction_phase(false);
		assert_ok!(Funding::stop_bidding(RuntimeOrigin::signed(ALICE)));
		assert!(!ActiveBidder::<Test>::get(ALICE));

		// Alice should be able to redeem while in the auction phase because she is not bidding
		MockEpochInfo::set_is_auction_phase(true);
		assert_ok!(Funding::redeem(RuntimeOrigin::signed(ALICE), AMOUNT.into(), ETH_DUMMY_ADDR),);
	});
}

#[test]
fn test_redeem_all() {
	new_test_ext().execute_with(|| {
		const AMOUNT: u128 = 100;
		const BOND: u128 = 55;

		// Add some funds.
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			AMOUNT,
			ETH_ZERO_ADDRESS,
			TX_HASH
		));

		// Alice becomes an authority.
		Bonder::<Test>::update_bond(&ALICE, BOND);

		// Redeem all available funds.
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			RedemptionAmount::Max,
			ETH_DUMMY_ADDR
		));
		assert_eq!(Flip::total_balance_of(&ALICE), BOND);

		// We should have a redemption for the full funded amount minus the bond.
		assert_event_sequence!(
			Test,
			RuntimeEvent::System(frame_system::Event::NewAccount { account: ALICE }),
			RuntimeEvent::Funding(crate::Event::Funded {
				account_id: ALICE,
				tx_hash: TX_HASH,
				funds_added: AMOUNT,
				total_balance: AMOUNT
			})
		);
	});
}

#[test]
fn test_check_withdrawal_address() {
	new_test_ext().execute_with(|| {
		const AMOUNT: u128 = 45;
		const DIFFERENT_ETH_ADDR: EthereumAddress = [45u8; 20];
		// Case: No account and no address provided
		assert!(Pallet::<Test>::check_withdrawal_address(&ALICE, ETH_ZERO_ADDRESS, AMOUNT).is_ok());
		assert!(!WithdrawalAddresses::<Test>::contains_key(ALICE));
		assert!(!FailedFundingAttempts::<Test>::contains_key(ALICE));
		// Case: No account and provided withdrawal address
		assert_ok!(Pallet::<Test>::check_withdrawal_address(&ALICE, ETH_DUMMY_ADDR, AMOUNT));
		let withdrawal_address = WithdrawalAddresses::<Test>::get(ALICE);
		assert!(withdrawal_address.is_some());
		assert_eq!(withdrawal_address.unwrap(), ETH_DUMMY_ADDR);
		// Case: User has already funded with a different address
		Pallet::<Test>::add_funds_to_account(&ALICE, AMOUNT);
		assert!(
			Pallet::<Test>::check_withdrawal_address(&ALICE, DIFFERENT_ETH_ADDR, AMOUNT).is_err()
		);
		let funding_attempts = FailedFundingAttempts::<Test>::get(ALICE);
		assert_eq!(funding_attempts.len(), 1);
		let funding_attempt = funding_attempts.first();
		assert_eq!(funding_attempt.unwrap().0, DIFFERENT_ETH_ADDR);
		assert_eq!(funding_attempt.unwrap().1, AMOUNT);
		for e in System::events().into_iter().map(|e| e.event) {
			println!("{e:?}");
		}
		assert_event_sequence!(
			Test,
			RuntimeEvent::System(frame_system::Event::NewAccount { account: ALICE }),
			RuntimeEvent::Funding(crate::Event::FailedFundingAttempt {
				account_id: ALICE,
				withdrawal_address: DIFFERENT_ETH_ADDR,
				amount: AMOUNT
			})
		);
		// Case: User funds again with the same address
		assert!(Pallet::<Test>::check_withdrawal_address(&ALICE, ETH_DUMMY_ADDR, AMOUNT).is_ok());
	});
}

#[test]
fn redeem_with_withdrawal_address() {
	new_test_ext().execute_with(|| {
		const AMOUNT: u128 = 45;
		const WRONG_ETH_ADDR: EthereumAddress = [45u8; 20];
		// Add some funds.
		assert_ok!(Funding::funded(RuntimeOrigin::root(), ALICE, AMOUNT, ETH_DUMMY_ADDR, TX_HASH));
		// Redeem it - expect to fail because the address is different
		assert_noop!(
			Funding::redeem(RuntimeOrigin::signed(ALICE), AMOUNT.into(), WRONG_ETH_ADDR),
			<Error<Test>>::WithdrawalAddressRestricted
		);
		// Try it again with the right address - expect to succeed
		assert_ok!(Funding::redeem(RuntimeOrigin::signed(ALICE), AMOUNT.into(), ETH_DUMMY_ADDR));
	});
}

#[test]
fn cannot_redeem_to_zero_address() {
	new_test_ext().execute_with(|| {
		const AMOUNT: u128 = 45;
		const ETH_ZERO_ADDRESS: EthereumAddress = [0xff; 20];
		// Add some funds, we use the zero address here to denote that we should be
		// able to redeem to any address in future
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			AMOUNT,
			ETH_ZERO_ADDRESS,
			TX_HASH
		));
		// Redeem it - expect to fail because the address is the zero address
		assert_noop!(
			Funding::redeem(RuntimeOrigin::signed(ALICE), AMOUNT.into(), ETH_ZERO_ADDRESS),
			<Error<Test>>::InvalidRedemption
		);
		// Try it again with a non-zero address - expect to succeed
		assert_ok!(Funding::redeem(RuntimeOrigin::signed(ALICE), AMOUNT.into(), ETH_DUMMY_ADDR));
	});
}

#[test]
fn redemption_expiry_removes_redemption() {
	new_test_ext().execute_with(|| {
		const AMOUNT: u128 = 45;

		assert_ok!(Funding::funded(RuntimeOrigin::root(), ALICE, AMOUNT, ETH_DUMMY_ADDR, TX_HASH));

		assert_ok!(Funding::redeem(RuntimeOrigin::signed(ALICE), AMOUNT.into(), ETH_DUMMY_ADDR));
		assert_noop!(
			Funding::redeem(RuntimeOrigin::signed(ALICE), AMOUNT.into(), ETH_DUMMY_ADDR),
			Error::<Test>::PendingRedemption
		);

		assert_ok!(Funding::redemption_expired(RuntimeOrigin::root(), ALICE, Default::default()));

		assert_noop!(
			Funding::redeemed(RuntimeOrigin::root(), ALICE, AMOUNT, TX_HASH),
			Error::<Test>::NoPendingRedemption
		);

		// Success, can request redemption again since the last one expired.
		assert_ok!(Funding::redeem(RuntimeOrigin::signed(ALICE), AMOUNT.into(), ETH_DUMMY_ADDR));
	});
}

#[test]
fn fund_with_provided_withdrawal_only_on_first_attempt() {
	// Check if the branching of the funding process is working probably
	new_test_ext().execute_with(|| {
		const AMOUNT: u128 = 45;
		// Add some funds with no withdrawal address
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			AMOUNT,
			ETH_ZERO_ADDRESS,
			TX_HASH
		));
		// Add some funds again with an provided withdrawal address
		assert_ok!(Funding::funded(RuntimeOrigin::root(), ALICE, AMOUNT, ETH_DUMMY_ADDR, TX_HASH));
		// Expect an failed funding event to be fired but no funding event
		assert_event_sequence!(
			Test,
			RuntimeEvent::System(frame_system::Event::NewAccount { account: ALICE }),
			RuntimeEvent::Funding(crate::Event::Funded {
				account_id: ALICE,
				tx_hash: TX_HASH,
				funds_added: AMOUNT,
				total_balance: AMOUNT
			}),
			RuntimeEvent::Funding(crate::Event::FailedFundingAttempt {
				account_id: ALICE,
				withdrawal_address: ETH_DUMMY_ADDR,
				amount: AMOUNT
			})
		);
	});
}

#[test]
fn maintenance_mode_blocks_redemption_requests() {
	new_test_ext().execute_with(|| {
		MockSystemStateInfo::set_maintenance(true);
		assert_noop!(
			Funding::redeem(RuntimeOrigin::signed(ALICE), RedemptionAmount::Max, ETH_DUMMY_ADDR),
			DispatchError::Other("We are in maintenance!")
		);
		MockSystemStateInfo::set_maintenance(false);
	});
}
