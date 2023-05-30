use crate::{
	mock::*, pallet, ActiveBidder, CollectedWithdrawalTax, Error, EthereumAddress,
	PendingRedemptions, RedemptionAmount, WithdrawalTax,
};
use cf_test_utilities::assert_event_sequence;
use cf_traits::{
	mocks::{
		account_role_registry::MockAccountRoleRegistry, system_state_info::MockSystemStateInfo,
	},
	AccountRoleRegistry, Bonding,
};

use frame_support::{assert_noop, assert_ok};
use pallet_cf_flip::Bonder;
use sp_runtime::{
	traits::{BadOrigin, BlockNumberProvider},
	DispatchError,
};

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
fn test_start_and_stop_bidding() {
	new_test_ext().execute_with(|| {
		MockEpochInfo::add_authorities(ALICE);
		const AMOUNT: u128 = 100;

		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			AMOUNT,
			ETH_ZERO_ADDRESS,
			TX_HASH
		));

		// Not yet registered as validator.
		assert_noop!(Funding::stop_bidding(RuntimeOrigin::signed(ALICE)), BadOrigin);
		assert_noop!(Funding::start_bidding(RuntimeOrigin::signed(ALICE)), BadOrigin);

		assert!(!ActiveBidder::<Test>::get(ALICE));

		assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(
			&ALICE
		));

		assert!(!ActiveBidder::<Test>::get(ALICE));

		assert_noop!(
			Funding::stop_bidding(RuntimeOrigin::signed(ALICE)),
			<Error<Test>>::AlreadyNotBidding
		);

		assert!(!ActiveBidder::<Test>::get(ALICE));

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
		assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(
			&ALICE
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

#[test]
fn can_update_withdrawal_tax() {
	new_test_ext().execute_with(|| {
		let amount = 1_000;
		assert_eq!(WithdrawalTax::<Test>::get(), 0);
		assert_ok!(Funding::update_withdrawal_tax(RuntimeOrigin::root(), amount));
		assert_eq!(WithdrawalTax::<Test>::get(), amount);
		System::assert_last_event(RuntimeEvent::Funding(
			crate::Event::<Test>::WithdrawalTaxAmountUpdated { amount },
		));
	});
}

#[test]
fn cannot_redeem_lower_than_withdrawal_tax() {
	new_test_ext().execute_with(|| {
		let amount = 1_000;
		assert_ok!(Funding::update_withdrawal_tax(RuntimeOrigin::root(), amount));
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			amount,
			Default::default(),
			Default::default(),
		));

		// Redemtion amount must be larger than the Withdrawal Tax
		assert_noop!(
			Funding::redeem(
				RuntimeOrigin::signed(ALICE),
				RedemptionAmount::Exact(amount),
				Default::default(),
			),
			crate::Error::<Test>::RedemptionAmountTooLow
		);

		// Reduce the withdrawal tax
		assert_ok!(Funding::update_withdrawal_tax(RuntimeOrigin::root(), amount - 1));

		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			RedemptionAmount::Exact(amount),
			Default::default()
		));
	});
}

#[test]
fn withdrawal_tax_is_collected_on_withdrawal() {
	new_test_ext().execute_with(|| {
		let tax = 1_000;
		let amount = 5_000;
		assert_ok!(Funding::update_withdrawal_tax(RuntimeOrigin::root(), tax));
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			1_000_000,
			Default::default(),
			Default::default(),
		));

		let previous_total_issuance = Flip::total_issuance();
		let previous_offchain = Flip::offchain_funds();

		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			RedemptionAmount::Exact(tax + amount),
			Default::default()
		));
		// Tell contract to send (TotalAmount - tax)
		assert_eq!(MockBroadcaster::received_requests(), vec![amount]);

		assert_ok!(Funding::redeemed(
			RuntimeOrigin::root(),
			ALICE,
			tax + amount,
			Default::default()
		));

		// Tax is collected
		let now = System::current_block_number();
		assert_eq!(CollectedWithdrawalTax::<Test>::get(now), tax);

		// Tax is burned
		assert_eq!(Flip::total_issuance(), previous_total_issuance - tax);
		// Total - tax is bridged out.
		assert_eq!(Flip::offchain_funds(), previous_offchain + amount);

		// More redeem add to the collected tax
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			RedemptionAmount::Exact(amount),
			Default::default()
		));
		assert_ok!(Funding::redeemed(RuntimeOrigin::root(), ALICE, amount, Default::default()));
		assert_eq!(CollectedWithdrawalTax::<Test>::get(now), tax * 2);

		// Collected tax is stored in the correct block
		System::set_block_number(now + 1);
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			RedemptionAmount::Exact(amount),
			Default::default()
		));
		assert_ok!(Funding::redeemed(RuntimeOrigin::root(), ALICE, amount, Default::default()));
		assert_eq!(CollectedWithdrawalTax::<Test>::get(now), tax * 2);
		assert_eq!(CollectedWithdrawalTax::<Test>::get(now + 1), tax);
	});
}
