use crate::{
	mock::*, pallet, ActiveBidder, BoundExecutorAddress, Error, EthereumAddress,
	PendingRedemptions, RedemptionAmount, RedemptionTax, RestrictedAddresses, RestrictedBalances,
};
use cf_primitives::FlipBalance;
use cf_test_utilities::assert_event_sequence;
use cf_traits::{
	mocks::account_role_registry::MockAccountRoleRegistry, AccountInfo, AccountRoleRegistry,
	Bonding, SetSafeMode,
};
use sp_core::H160;

use crate::BoundRedeemAddress;
use frame_support::{assert_noop, assert_ok};
use pallet_cf_flip::Bonder;
use sp_runtime::{traits::BadOrigin, DispatchError};

type FlipError = pallet_cf_flip::Error<Test>;

const ETH_DUMMY_ADDR: EthereumAddress = H160([42u8; 20]);
const ETH_ZERO_ADDRESS: EthereumAddress = H160([0u8; 20]);
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
		const AMOUNT_A1: u128 = 500;
		const AMOUNT_A2: u128 = 200;
		const REDEMPTION_A: u128 = 400;
		const AMOUNT_B: u128 = 800;
		const REDEMPTION_B: RedemptionAmount<FlipBalance> = RedemptionAmount::Max;

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
			ETH_DUMMY_ADDR,
			Default::default(),
		));
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(BOB),
			REDEMPTION_B,
			ETH_DUMMY_ADDR,
			Default::default()
		));

		// Make sure it was subtracted.
		assert_eq!(
			Flip::total_balance_of(&ALICE),
			AMOUNT_A1 + AMOUNT_A2 - REDEMPTION_A - REDEMPTION_TAX
		);
		assert_eq!(Flip::total_balance_of(&BOB), 0);

		// Check the pending redemptions
		assert!(PendingRedemptions::<Test>::get(ALICE).is_some());
		assert!(PendingRedemptions::<Test>::get(BOB).is_some());

		// Two broadcasts should have been initiated by the two redemptions.
		assert_eq!(MockBroadcaster::received_requests().len(), 2);

		const TOTAL_A: u128 = AMOUNT_A1 + AMOUNT_A2;
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
				total_balance: TOTAL_A,
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
			Funding::redeem(
				RuntimeOrigin::signed(ALICE),
				AMOUNT.into(),
				ETH_DUMMY_ADDR,
				Default::default()
			),
			Error::<Test>::InsufficientBalance
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
				ETH_DUMMY_ADDR,
				Default::default()
			),
			Error::<Test>::BelowMinimumFunding
		);

		// Redeem FLIP from another account.
		assert_noop!(
			Funding::redeem(
				RuntimeOrigin::signed(BOB),
				AMOUNT.into(),
				ETH_DUMMY_ADDR,
				Default::default()
			),
			Error::<Test>::InsufficientBalance
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
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			amount_a1.into(),
			ETH_DUMMY_ADDR,
			Default::default()
		));

		// Redeeming the rest should not be possible yet.
		assert_noop!(
			Funding::redeem(
				RuntimeOrigin::signed(ALICE),
				amount_a1.into(),
				ETH_DUMMY_ADDR,
				Default::default()
			),
			<Error<Test>>::PendingRedemption
		);

		assert_ok!(Funding::redeemed(RuntimeOrigin::root(), ALICE, amount_a1, TX_HASH));
		assert!(PendingRedemptions::<Test>::get(&ALICE).is_none());

		// Should now be able to redeem the rest.
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			RedemptionAmount::Max,
			ETH_DUMMY_ADDR,
			Default::default()
		));

		assert_ok!(Funding::redeemed(
			RuntimeOrigin::root(),
			ALICE,
			amount_a2 - RedemptionTax::<Test>::get(),
			TX_HASH
		));
		assert!(PendingRedemptions::<Test>::get(&ALICE).is_none());

		// Remaining amount should be zero
		assert_eq!(Flip::total_balance_of(&ALICE), 0u128);
	});
}

#[test]
fn redemption_cannot_occur_without_funding_first() {
	new_test_ext().execute_with(|| {
		const FUNDING_AMOUNT: u128 = 45;
		const REDEEMED_AMOUNT: u128 = FUNDING_AMOUNT - REDEMPTION_TAX;

		// Account doesn't exist yet.
		assert!(!frame_system::Pallet::<Test>::account_exists(&ALICE));

		// Add some funds.
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			FUNDING_AMOUNT,
			ETH_ZERO_ADDRESS,
			TX_HASH
		));

		// The act of funding creates the account.
		assert!(frame_system::Pallet::<Test>::account_exists(&ALICE));

		// Redeem it.
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			RedemptionAmount::Max,
			ETH_DUMMY_ADDR,
			Default::default()
		));

		// Redeem should kick off a broadcast request.
		assert_eq!(MockBroadcaster::received_requests().len(), 1);

		// Invalid Redeemed Event from Ethereum: wrong account.
		assert_noop!(
			Funding::redeemed(RuntimeOrigin::root(), BOB, FUNDING_AMOUNT, TX_HASH),
			<Error<Test>>::NoPendingRedemption
		);

		// Valid Redeemed Event from Ethereum.
		assert_ok!(Funding::redeemed(RuntimeOrigin::root(), ALICE, REDEEMED_AMOUNT, TX_HASH));

		// The account balance is now zero, it should have been reaped.
		assert!(!frame_system::Pallet::<Test>::account_exists(&ALICE));

		assert_event_sequence!(
			Test,
			RuntimeEvent::System(frame_system::Event::NewAccount { account: ALICE }),
			RuntimeEvent::Funding(crate::Event::Funded {
				account_id: ALICE,
				tx_hash: TX_HASH,
				funds_added: FUNDING_AMOUNT,
				total_balance: FUNDING_AMOUNT
			}),
			RuntimeEvent::Funding(crate::Event::RedemptionRequested {
				account_id: ALICE,
				amount: REDEEMED_AMOUNT,
				broadcast_id: 0,
				expiry_time: 10,
			}),
			RuntimeEvent::System(frame_system::Event::KilledAccount { account: ALICE }),
			RuntimeEvent::Funding(crate::Event::RedemptionSettled(ALICE, REDEEMED_AMOUNT))
		);
	});
}

#[test]
fn cannot_redeem_bond() {
	new_test_ext().execute_with(|| {
		assert_ok!(Funding::update_redemption_tax(RuntimeOrigin::root(), 0));
		assert_ok!(Funding::update_minimum_funding(RuntimeOrigin::root(), 1));
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
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(BOB),
			AMOUNT.into(),
			ETH_DUMMY_ADDR,
			Default::default()
		));
		assert_noop!(
			Funding::redeem(
				RuntimeOrigin::signed(ALICE),
				AMOUNT.into(),
				ETH_DUMMY_ADDR,
				Default::default()
			),
			FlipError::InsufficientLiquidity
		);

		// Alice *can* withdraw 100
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			(AMOUNT - BOND).into(),
			ETH_DUMMY_ADDR,
			Default::default()
		));

		// Even if she redeems, the remaining 100 are blocked
		assert_ok!(Funding::redeemed(RuntimeOrigin::root(), ALICE, AMOUNT - BOND, TX_HASH));
		assert_noop!(
			Funding::redeem(
				RuntimeOrigin::signed(ALICE),
				1.into(),
				ETH_DUMMY_ADDR,
				Default::default()
			),
			FlipError::InsufficientLiquidity
		);

		// Once she is no longer bonded, Alice can redeem her funds.
		Bonder::<Test>::update_bond(&ALICE, 0u128);
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			BOND.into(),
			ETH_DUMMY_ADDR,
			Default::default()
		));
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
			Funding::redeem(
				RuntimeOrigin::signed(ALICE),
				RedemptionAmount::Max,
				ETH_DUMMY_ADDR,
				Default::default()
			),
			<Error<Test>>::AuctionPhase
		);

		// Stop bidding for Alice (must be done outside of the auction phase)
		MockEpochInfo::set_is_auction_phase(false);
		assert_ok!(Funding::stop_bidding(RuntimeOrigin::signed(ALICE)));
		assert!(!ActiveBidder::<Test>::get(ALICE));

		// Alice should be able to redeem while in the auction phase because she is not bidding
		MockEpochInfo::set_is_auction_phase(true);
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			RedemptionAmount::Max,
			ETH_DUMMY_ADDR,
			Default::default()
		),);
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
			ETH_DUMMY_ADDR,
			Default::default()
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
fn redemption_expiry_removes_redemption() {
	new_test_ext().execute_with(|| {
		const TOTAL_FUNDS: u128 = 100;
		const TO_REDEEM: u128 = 45;
		const RESTRICTED_AMOUNT: u128 = 60;
		const RESTRICTED_ADDRESS: EthereumAddress = EthereumAddress::repeat_byte(0x02);

		RestrictedAddresses::<Test>::insert(RESTRICTED_ADDRESS, ());
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			RESTRICTED_AMOUNT,
			RESTRICTED_ADDRESS,
			TX_HASH
		));
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			TOTAL_FUNDS - RESTRICTED_AMOUNT,
			ETH_DUMMY_ADDR,
			TX_HASH
		));
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			TO_REDEEM.into(),
			RESTRICTED_ADDRESS,
			Default::default()
		));
		assert_noop!(
			Funding::redeem(
				RuntimeOrigin::signed(ALICE),
				TO_REDEEM.into(),
				ETH_DUMMY_ADDR,
				Default::default()
			),
			Error::<Test>::PendingRedemption
		);

		// Restricted funds and total balance should have been reduced.
		assert_eq!(Flip::total_balance_of(&ALICE), TOTAL_FUNDS - REDEMPTION_TAX - TO_REDEEM);
		assert_eq!(
			*RestrictedBalances::<Test>::get(&ALICE).get(&RESTRICTED_ADDRESS).unwrap(),
			RESTRICTED_AMOUNT - REDEMPTION_TAX - TO_REDEEM
		);

		assert_ok!(Funding::redemption_expired(RuntimeOrigin::root(), ALICE, Default::default()));

		// Tax was paid, rest is returned.
		assert_eq!(Flip::total_balance_of(&ALICE), TOTAL_FUNDS - REDEMPTION_TAX);
		// Restricted funds are restricted again, minus redemption tax.
		assert_eq!(
			*RestrictedBalances::<Test>::get(&ALICE).get(&RESTRICTED_ADDRESS).unwrap(),
			RESTRICTED_AMOUNT - REDEMPTION_TAX
		);

		assert_noop!(
			Funding::redeemed(RuntimeOrigin::root(), ALICE, TOTAL_FUNDS, TX_HASH),
			Error::<Test>::NoPendingRedemption
		);

		// Success, can request redemption again since the last one expired.
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			TO_REDEEM.into(),
			RESTRICTED_ADDRESS,
			Default::default()
		));
	});
}

#[test]
fn restore_restricted_balance_when_redemption_expires() {
	const TOTAL_FUNDS: u128 = 100;
	const RESTRICTED_AMOUNT: u128 = 60;
	const RESTRICTED_ADDRESS: EthereumAddress = EthereumAddress::repeat_byte(0x02);

	#[track_caller]
	fn do_test(redeem_amount: RedemptionAmount<u128>) {
		new_test_ext().execute_with(|| {
			RestrictedAddresses::<Test>::insert(RESTRICTED_ADDRESS, ());
			assert_ok!(Funding::funded(
				RuntimeOrigin::root(),
				ALICE,
				RESTRICTED_AMOUNT,
				RESTRICTED_ADDRESS,
				TX_HASH
			));
			assert_ok!(Funding::funded(
				RuntimeOrigin::root(),
				ALICE,
				TOTAL_FUNDS - RESTRICTED_AMOUNT,
				ETH_DUMMY_ADDR,
				TX_HASH
			));
			assert_ok!(Funding::redeem(
				RuntimeOrigin::signed(ALICE),
				redeem_amount,
				RESTRICTED_ADDRESS,
				Default::default()
			));

			// Restricted funds and total balance should have been reduced.
			assert!(Flip::total_balance_of(&ALICE) < TOTAL_FUNDS);

			assert_ok!(Funding::redemption_expired(
				RuntimeOrigin::root(),
				ALICE,
				Default::default()
			));

			assert_eq!(
				Flip::total_balance_of(&ALICE),
				TOTAL_FUNDS - REDEMPTION_TAX,
				"Expected the full balance, minus redemption tax, to be restored to the account"
			);
			let new_restricted_balance = *RestrictedBalances::<Test>::get(&ALICE)
				.get(&RESTRICTED_ADDRESS)
				.expect("Expected the restricted balance to be restored to the restricted address");
			assert_eq!(
				new_restricted_balance,
				RESTRICTED_AMOUNT - REDEMPTION_TAX,
				"Expected the restricted balance to be restored excluding the redemption tax",
			);
		});
	}

	// Redeem all.
	do_test(RedemptionAmount::Max);
	// Redeem more than the restricted balance.
	do_test(RedemptionAmount::Exact(RESTRICTED_AMOUNT + REDEMPTION_TAX * 2));
	// Redeem exactly the restricted balance.
	do_test(RedemptionAmount::Exact(RESTRICTED_AMOUNT));
	// Redeem a little less than the restricted balance.
	do_test(RedemptionAmount::Exact(RESTRICTED_AMOUNT - 1));
	// Redeem signficantly less than the restricted balance.
	do_test(RedemptionAmount::Exact(RESTRICTED_AMOUNT - REDEMPTION_TAX * 2));
}

#[test]
fn runtime_safe_mode_blocks_redemption_requests() {
	new_test_ext().execute_with(|| {
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			1_000,
			Default::default(),
			Default::default(),
		));

		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_red();
		assert_noop!(
			Funding::redeem(
				RuntimeOrigin::signed(ALICE),
				RedemptionAmount::Max,
				ETH_DUMMY_ADDR,
				Default::default()
			),
			Error::<Test>::RedeemDisabled
		);

		<MockRuntimeSafeMode as SetSafeMode<MockRuntimeSafeMode>>::set_code_green();
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			RedemptionAmount::Max,
			ETH_DUMMY_ADDR,
			Default::default()
		));
	});
}

#[test]
fn restricted_funds_getting_recorded() {
	new_test_ext().execute_with(|| {
		const AMOUNT: u128 = 45;
		const RESTRICTED_ADDRESS: EthereumAddress = H160([0xff; 20]);

		// Add Address to list of restricted contracts
		RestrictedAddresses::<Test>::insert(RESTRICTED_ADDRESS, ());

		// Add some funds, we use the zero address here to denote that we should be
		// able to redeem to any address in future
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			AMOUNT,
			RESTRICTED_ADDRESS,
			TX_HASH
		));

		assert_eq!(
			RestrictedBalances::<Test>::get(ALICE).get(&RESTRICTED_ADDRESS).unwrap(),
			&AMOUNT
		);
	});
}

#[test]
fn can_update_redemption_tax() {
	new_test_ext().execute_with(|| {
		let amount = 1_000;
		assert_ok!(Funding::update_minimum_funding(RuntimeOrigin::root(), amount + 1));
		assert_ok!(Funding::update_redemption_tax(RuntimeOrigin::root(), amount));
		assert_eq!(RedemptionTax::<Test>::get(), amount);
		System::assert_last_event(RuntimeEvent::Funding(
			crate::Event::<Test>::RedemptionTaxAmountUpdated { amount },
		));
	});
}

#[test]
fn restricted_funds_pay_redemption_tax() {
	new_test_ext().execute_with(|| {
		const RESTRICTED_ADDRESS: EthereumAddress = H160([0x42; 20]);
		const RESTRICTED_AMOUNT: FlipBalance = 50;
		const UNRESTRICTED_ADDRESS: EthereumAddress = H160([0x01; 20]);
		const UNRESTRICTED_AMOUNT: FlipBalance = 20;
		const REDEEM_AMOUNT: FlipBalance = 10;

		RestrictedAddresses::<Test>::insert(RESTRICTED_ADDRESS, ());
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			RESTRICTED_AMOUNT,
			RESTRICTED_ADDRESS,
			TX_HASH
		));
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			UNRESTRICTED_AMOUNT,
			UNRESTRICTED_ADDRESS,
			TX_HASH
		));

		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			REDEEM_AMOUNT.into(),
			RESTRICTED_ADDRESS,
			Default::default()
		));
		assert_ok!(Funding::redeemed(RuntimeOrigin::root(), ALICE, REDEEM_AMOUNT, TX_HASH));
		assert_eq!(
			*RestrictedBalances::<Test>::get(ALICE).get(&RESTRICTED_ADDRESS).unwrap(),
			RESTRICTED_AMOUNT - REDEEM_AMOUNT - REDEMPTION_TAX
		);
	});
}

#[test]
fn redemption_tax_cannot_be_larger_than_minimum_funding() {
	new_test_ext().execute_with(|| {
		let amount = 1_000;
		assert_ok!(Funding::update_minimum_funding(RuntimeOrigin::root(), amount));
		assert_noop!(
			Funding::update_redemption_tax(RuntimeOrigin::root(), amount),
			Error::<Test>::InvalidRedemptionTaxUpdate
		);
	});
}

#[test]
fn vesting_contracts_test_case() {
	new_test_ext().execute_with(|| {
		// Contracts
		const VESTING_CONTRACT_1: EthereumAddress = H160([0x01; 20]);
		const VESTING_CONTRACT_2: EthereumAddress = H160([0x02; 20]);
		const UNRESTRICTED_ADDRESS: EthereumAddress = H160([0x03; 20]);
		// Balances
		const CONTRACT_1_FUNDS: u128 = 200;
		const CONTRACT_2_FUNDS: u128 = 800;
		const EARNED_REWARDS: u128 = 100;
		// Add contract address to list of restricted contracts
		RestrictedAddresses::<Test>::insert(VESTING_CONTRACT_1, ());
		RestrictedAddresses::<Test>::insert(VESTING_CONTRACT_2, ());
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			CONTRACT_1_FUNDS,
			VESTING_CONTRACT_1,
			TX_HASH
		));
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			CONTRACT_2_FUNDS,
			VESTING_CONTRACT_2,
			TX_HASH
		));
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			EARNED_REWARDS,
			UNRESTRICTED_ADDRESS,
			TX_HASH
		));
		// Because 100 is available this should fail
		assert_noop!(
			Funding::redeem(
				RuntimeOrigin::signed(ALICE),
				200.into(),
				UNRESTRICTED_ADDRESS,
				Default::default()
			),
			Error::<Test>::InsufficientUnrestrictedFunds
		);
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			50.into(),
			UNRESTRICTED_ADDRESS,
			Default::default()
		));
		assert_ok!(Funding::redeemed(RuntimeOrigin::root(), ALICE, 50, TX_HASH));
		// Try to redeem 100 from contract 1
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			100.into(),
			VESTING_CONTRACT_1,
			Default::default()
		));
		assert_ok!(Funding::redeemed(RuntimeOrigin::root(), ALICE, 100, TX_HASH));
		// Try to redeem 400 from contract 2
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			400.into(),
			VESTING_CONTRACT_2,
			Default::default()
		));
		assert_ok!(Funding::redeemed(RuntimeOrigin::root(), ALICE, 400, TX_HASH));
	});
}

#[test]
fn can_withdraw_unrestricted_to_restricted() {
	new_test_ext().execute_with(|| {
		// Contracts
		const RESTRICTED_ADDRESS_1: EthereumAddress = H160([0x01; 20]);
		const RESTRICTED_ADDRESS_2: EthereumAddress = H160([0x02; 20]);
		const UNRESTRICTED_ADDRESS: EthereumAddress = H160([0x03; 20]);
		// Balances
		const AMOUNT: u128 = 100;
		// Add restricted addresses.
		RestrictedAddresses::<Test>::insert(RESTRICTED_ADDRESS_1, ());
		RestrictedAddresses::<Test>::insert(RESTRICTED_ADDRESS_2, ());
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			AMOUNT,
			UNRESTRICTED_ADDRESS,
			TX_HASH
		));
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			AMOUNT,
			RESTRICTED_ADDRESS_2,
			TX_HASH
		));
		// Funds are not restricted, this should be ok.
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			(AMOUNT - RedemptionTax::<Test>::get()).into(),
			RESTRICTED_ADDRESS_1,
			Default::default()
		));
	});
}

#[test]
fn can_withdrawal_also_free_funds_to_restricted_address() {
	new_test_ext().execute_with(|| {
		const RESTRICTED_ADDRESS_1: EthereumAddress = H160([0x01; 20]);
		const UNRESTRICTED_ADDRESS: EthereumAddress = H160([0x03; 20]);
		const AMOUNT_1: u128 = 100;
		const AMOUNT_2: u128 = 50;
		RestrictedAddresses::<Test>::insert(RESTRICTED_ADDRESS_1, ());
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			AMOUNT_1,
			RESTRICTED_ADDRESS_1,
			TX_HASH
		));
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			AMOUNT_2,
			UNRESTRICTED_ADDRESS,
			TX_HASH
		));
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			RedemptionAmount::Max,
			RESTRICTED_ADDRESS_1,
			Default::default()
		));
		assert_eq!(RestrictedBalances::<Test>::get(ALICE).get(&RESTRICTED_ADDRESS_1), None);
	});
}

#[test]
fn can_only_redeem_funds_to_bound_address() {
	new_test_ext().execute_with(|| {
		const RESTRICTED_ADDRESS_1: EthereumAddress = H160([0x01; 20]);
		const BOUND_ADDRESS: EthereumAddress = H160([0x02; 20]);
		const UNRESTRICTED_ADDRESS: EthereumAddress = H160([0x03; 20]);
		const AMOUNT: u128 = 100;
		RestrictedAddresses::<Test>::insert(RESTRICTED_ADDRESS_1, ());
		BoundRedeemAddress::<Test>::insert(ALICE, BOUND_ADDRESS);
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			AMOUNT,
			UNRESTRICTED_ADDRESS,
			TX_HASH
		));
		assert_noop!(
			Funding::redeem(
				RuntimeOrigin::signed(ALICE),
				AMOUNT.into(),
				UNRESTRICTED_ADDRESS,
				Default::default()
			),
			Error::<Test>::AccountBindingRestrictionViolated
		);
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			AMOUNT,
			UNRESTRICTED_ADDRESS,
			TX_HASH
		));
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			AMOUNT.into(),
			BOUND_ADDRESS,
			Default::default()
		));
	});
}

#[test]
fn redeem_funds_until_restricted_balance_is_zero_and_then_redeem_to_redeem_address() {
	new_test_ext().execute_with(|| {
		const RESTRICTED_ADDRESS: EthereumAddress = H160([0x01; 20]);
		const REDEEM_ADDRESS: EthereumAddress = H160([0x02; 20]);
		const UNRESTRICTED_ADDRESS: EthereumAddress = H160([0x03; 20]);
		const AMOUNT: u128 = 100;
		RestrictedAddresses::<Test>::insert(RESTRICTED_ADDRESS, ());
		BoundRedeemAddress::<Test>::insert(ALICE, REDEEM_ADDRESS);
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			AMOUNT,
			UNRESTRICTED_ADDRESS,
			TX_HASH
		));
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			AMOUNT,
			RESTRICTED_ADDRESS,
			TX_HASH
		));
		assert_ok!(Funding::funded(RuntimeOrigin::root(), ALICE, AMOUNT, REDEEM_ADDRESS, TX_HASH));
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			(AMOUNT).into(),
			RESTRICTED_ADDRESS,
			Default::default()
		));
		assert_ok!(Funding::redeemed(RuntimeOrigin::root(), ALICE, AMOUNT, TX_HASH));
		// Redeem to an unrestricted address should fail because the account has a redeem address.
		assert_noop!(
			Funding::redeem(
				RuntimeOrigin::signed(ALICE),
				(AMOUNT).into(),
				UNRESTRICTED_ADDRESS,
				Default::default()
			),
			Error::<Test>::AccountBindingRestrictionViolated
		);
		// Redeem the rest of the existing funds to the redeem address.
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			(AMOUNT).into(),
			REDEEM_ADDRESS,
			Default::default()
		));
	});
}

#[test]
fn redeem_funds_to_restricted_address_overrides_bound_and_executor_restrictions() {
	new_test_ext().execute_with(|| {
		const RESTRICTED_ADDRESS: EthereumAddress = H160([0x01; 20]);
		const REDEEM_ADDRESS: EthereumAddress = H160([0x02; 20]);
		const EXECUTOR_ADDRESS: EthereumAddress = H160([0x04; 20]);
		const RANDOM_ADDRESS: EthereumAddress = H160([0x12; 20]);
		const AMOUNT: u128 = 100;
		RestrictedAddresses::<Test>::insert(RESTRICTED_ADDRESS, ());
		BoundRedeemAddress::<Test>::insert(ALICE, REDEEM_ADDRESS);
		BoundExecutorAddress::<Test>::insert(ALICE, EXECUTOR_ADDRESS);

		assert_ok!(Funding::funded(RuntimeOrigin::root(), ALICE, AMOUNT, REDEEM_ADDRESS, TX_HASH));
		assert_ok!(Funding::funded(RuntimeOrigin::root(), ALICE, AMOUNT, REDEEM_ADDRESS, TX_HASH));
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			AMOUNT,
			RESTRICTED_ADDRESS,
			TX_HASH
		));

		// Redeem using a wrong executor should fail because we have bounded executor address
		assert_noop!(
			Funding::redeem(
				RuntimeOrigin::signed(ALICE),
				(AMOUNT).into(),
				REDEEM_ADDRESS,
				Default::default()
			),
			Error::<Test>::ExecutorBindingRestrictionViolated
		);
		// Redeem using correct redeem and executor should complete succesfully
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			(AMOUNT).into(),
			REDEEM_ADDRESS,
			Some(EXECUTOR_ADDRESS)
		));
		assert_ok!(Funding::redeemed(RuntimeOrigin::root(), ALICE, AMOUNT, TX_HASH));
		// Redeem using restricted address should complete even with wrong executor and bound redeem
		// address
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			(AMOUNT).into(),
			RESTRICTED_ADDRESS,
			Some(RANDOM_ADDRESS)
		));
		assert_ok!(Funding::redeemed(RuntimeOrigin::root(), ALICE, AMOUNT, TX_HASH));
	});
}

#[cfg(test)]
mod test_restricted_balances {
	use sp_core::H160;

	use super::*;

	const RESTRICTED_ADDRESS_1: EthereumAddress = H160([0x01; 20]);
	const RESTRICTED_BALANCE_1: u128 = 200;
	const RESTRICTED_ADDRESS_2: EthereumAddress = H160([0x02; 20]);
	const RESTRICTED_BALANCE_2: u128 = 800;
	const UNRESTRICTED_ADDRESS: EthereumAddress = H160([0x03; 20]);
	const UNRESTRICTED_BALANCE: u128 = 100;
	const TOTAL_BALANCE: u128 = RESTRICTED_BALANCE_1 + RESTRICTED_BALANCE_2 + UNRESTRICTED_BALANCE;

	const NO_BOND: u128 = 0;
	const LOW_BOND: u128 = UNRESTRICTED_BALANCE + RESTRICTED_BALANCE_1 + 50;
	const MID_BOND: u128 = UNRESTRICTED_BALANCE + RESTRICTED_BALANCE_2 + 50;
	const HIGH_BOND: u128 = RESTRICTED_BALANCE_1 + RESTRICTED_BALANCE_2 + 50;

	#[track_caller]
	fn run_test<E: Into<DispatchError>>(
		bond: FlipBalance,
		redeem_amount: RedemptionAmount<FlipBalance>,
		bound_redeem_address: EthereumAddress,
		maybe_error: Option<E>,
	) {
		new_test_ext().execute_with(|| {
			RestrictedAddresses::<Test>::insert(RESTRICTED_ADDRESS_1, ());
			RestrictedAddresses::<Test>::insert(RESTRICTED_ADDRESS_2, ());

			for (address, amount) in [
				(RESTRICTED_ADDRESS_1, RESTRICTED_BALANCE_1),
				(RESTRICTED_ADDRESS_2, RESTRICTED_BALANCE_2),
				(UNRESTRICTED_ADDRESS, UNRESTRICTED_BALANCE + REDEMPTION_TAX),
			] {
				assert_ok!(Funding::funded(
					RuntimeOrigin::root(),
					ALICE,
					amount,
					address,
					Default::default(),
				));
			}

			Bonder::<Test>::update_bond(&ALICE, bond);

			let initial_balance = Flip::balance(&ALICE);
			assert_eq!(initial_balance, TOTAL_BALANCE + REDEMPTION_TAX);

			match maybe_error {
				None => {
					assert_ok!(Funding::redeem(
						RuntimeOrigin::signed(ALICE),
						redeem_amount,
						bound_redeem_address,
						Default::default()
					));
					let expected_redeemed_amount =
						initial_balance - Flip::balance(&ALICE) - RedemptionTax::<Test>::get();
					assert!(matches!(
						cf_test_utilities::last_event::<Test>(),
						RuntimeEvent::Funding(crate::Event::RedemptionRequested {
							account_id,
							amount,
							..
						}) if account_id == ALICE && amount == expected_redeemed_amount));
				},
				Some(e) => {
					assert_noop!(
						Funding::redeem(
							RuntimeOrigin::signed(ALICE),
							redeem_amount,
							bound_redeem_address,
							Default::default()
						),
						e.into(),
					);
				},
			}
		});
	}

	/// Takes a test identifier, a bond amount, and a list of redemption expressions, where each
	/// expression is of the form `(amount, redeem_address, maybe_error)`, and
	/// `maybe_err` is Some(error) when an error is expected.
	macro_rules! test_restricted_balances {
		( $case:ident, $bond:expr, $( $spec:expr, )+ ) => {
			#[test]
			fn $case() {
				$(
					std::panic::catch_unwind(||
						run_test(
							$bond,
							$spec.0,
							$spec.1,
							$spec.2,
						)
					)
					.unwrap_or_else(|_| {
						let spec = stringify!($spec);
						panic!("Test failed with {spec}");
					});
				)+
			}
		};
	}

	test_restricted_balances![
		up_to_100_can_be_claimed_to_any_address,
		NO_BOND,
		(RedemptionAmount::Exact(MIN_FUNDING), UNRESTRICTED_ADDRESS, None::<Error<Test>>),
		(RedemptionAmount::Exact(MIN_FUNDING), RESTRICTED_ADDRESS_1, None::<Error<Test>>),
		(RedemptionAmount::Exact(MIN_FUNDING), RESTRICTED_ADDRESS_2, None::<Error<Test>>),
		(RedemptionAmount::Exact(UNRESTRICTED_BALANCE), UNRESTRICTED_ADDRESS, None::<Error<Test>>),
		(RedemptionAmount::Exact(UNRESTRICTED_BALANCE), RESTRICTED_ADDRESS_1, None::<Error<Test>>),
		(RedemptionAmount::Exact(UNRESTRICTED_BALANCE), RESTRICTED_ADDRESS_2, None::<Error<Test>>),
	];
	test_restricted_balances![
		restricted_funds_can_only_be_redeemed_to_restricted_addresses,
		NO_BOND,
		(
			RedemptionAmount::Exact(UNRESTRICTED_BALANCE + 1),
			UNRESTRICTED_ADDRESS,
			Some(Error::<Test>::InsufficientUnrestrictedFunds)
		),
		(
			RedemptionAmount::Exact(UNRESTRICTED_BALANCE + 1),
			RESTRICTED_ADDRESS_1,
			None::<Error<Test>>
		),
		(
			RedemptionAmount::Exact(UNRESTRICTED_BALANCE + 1),
			RESTRICTED_ADDRESS_2,
			None::<Error<Test>>
		),
		(
			RedemptionAmount::Exact(UNRESTRICTED_BALANCE + RESTRICTED_BALANCE_1),
			UNRESTRICTED_ADDRESS,
			Some(Error::<Test>::InsufficientUnrestrictedFunds)
		),
		(
			RedemptionAmount::Exact(UNRESTRICTED_BALANCE + RESTRICTED_BALANCE_1),
			RESTRICTED_ADDRESS_1,
			None::<Error<Test>>
		),
		(
			RedemptionAmount::Exact(UNRESTRICTED_BALANCE + RESTRICTED_BALANCE_1),
			RESTRICTED_ADDRESS_2,
			None::<Error<Test>>
		),
	];
	test_restricted_balances![
		higher_than_restricted_amount_1_can_only_be_redeemed_to_restricted_address_2,
		NO_BOND,
		(
			RedemptionAmount::Exact(UNRESTRICTED_BALANCE + RESTRICTED_BALANCE_1 + 1),
			UNRESTRICTED_ADDRESS,
			Some(Error::<Test>::InsufficientUnrestrictedFunds)
		),
		(
			RedemptionAmount::Exact(UNRESTRICTED_BALANCE + RESTRICTED_BALANCE_1 + 1),
			RESTRICTED_ADDRESS_1,
			Some(Error::<Test>::InsufficientUnrestrictedFunds)
		),
		(
			RedemptionAmount::Exact(UNRESTRICTED_BALANCE + RESTRICTED_BALANCE_1 + 1),
			RESTRICTED_ADDRESS_2,
			None::<Error<Test>>
		),
		(
			RedemptionAmount::Exact(UNRESTRICTED_BALANCE + RESTRICTED_BALANCE_2),
			UNRESTRICTED_ADDRESS,
			Some(Error::<Test>::InsufficientUnrestrictedFunds)
		),
		(
			RedemptionAmount::Exact(UNRESTRICTED_BALANCE + RESTRICTED_BALANCE_2),
			RESTRICTED_ADDRESS_1,
			Some(Error::<Test>::InsufficientUnrestrictedFunds)
		),
		(
			RedemptionAmount::Exact(UNRESTRICTED_BALANCE + RESTRICTED_BALANCE_2),
			RESTRICTED_ADDRESS_2,
			None::<Error<Test>>
		),
	];
	test_restricted_balances![
		redemptions_of_more_than_the_higher_restricted_amount_are_not_possible_to_any_address,
		NO_BOND,
		(
			RedemptionAmount::Exact(UNRESTRICTED_BALANCE + RESTRICTED_BALANCE_2 + 1),
			UNRESTRICTED_ADDRESS,
			Some(Error::<Test>::InsufficientUnrestrictedFunds)
		),
		(
			RedemptionAmount::Exact(UNRESTRICTED_BALANCE + RESTRICTED_BALANCE_2 + 1),
			RESTRICTED_ADDRESS_1,
			Some(Error::<Test>::InsufficientUnrestrictedFunds)
		),
		(
			RedemptionAmount::Exact(UNRESTRICTED_BALANCE + RESTRICTED_BALANCE_2 + 1),
			RESTRICTED_ADDRESS_2,
			Some(Error::<Test>::InsufficientUnrestrictedFunds)
		),
		(
			RedemptionAmount::<FlipBalance>::Exact(TOTAL_BALANCE),
			UNRESTRICTED_ADDRESS,
			Some(Error::<Test>::InsufficientUnrestrictedFunds)
		),
		(
			RedemptionAmount::<FlipBalance>::Exact(TOTAL_BALANCE),
			RESTRICTED_ADDRESS_1,
			Some(Error::<Test>::InsufficientUnrestrictedFunds)
		),
		(
			RedemptionAmount::<FlipBalance>::Exact(TOTAL_BALANCE),
			RESTRICTED_ADDRESS_2,
			Some(Error::<Test>::InsufficientUnrestrictedFunds)
		),
		(RedemptionAmount::<FlipBalance>::Max, UNRESTRICTED_ADDRESS, None::<Error<Test>>),
		(RedemptionAmount::<FlipBalance>::Max, RESTRICTED_ADDRESS_1, None::<Error<Test>>),
		(RedemptionAmount::<FlipBalance>::Max, RESTRICTED_ADDRESS_2, None::<Error<Test>>),
	];
	// With the low bond, the higher restricted balance is blocked by the bond.
	test_restricted_balances![
		bond_takes_precedence_over_restricted_balances_with_low_bond,
		LOW_BOND,
		(
			RedemptionAmount::<FlipBalance>::Exact(UNRESTRICTED_BALANCE),
			UNRESTRICTED_ADDRESS,
			None::<Error<Test>>
		),
		(
			RedemptionAmount::<FlipBalance>::Exact(RESTRICTED_BALANCE_1),
			RESTRICTED_ADDRESS_1,
			None::<Error<Test>>
		),
		(
			RedemptionAmount::<FlipBalance>::Exact(RESTRICTED_BALANCE_2),
			RESTRICTED_ADDRESS_2,
			Some(FlipError::InsufficientLiquidity)
		),
	];
	// With the mid-sized bond, both restricted balances are blocked by the bond.
	test_restricted_balances![
		bond_takes_precedence_over_restricted_balances_with_mid_bond,
		MID_BOND,
		(
			RedemptionAmount::<FlipBalance>::Exact(UNRESTRICTED_BALANCE),
			UNRESTRICTED_ADDRESS,
			None::<Error<Test>>
		),
		(
			RedemptionAmount::<FlipBalance>::Exact(RESTRICTED_BALANCE_1),
			RESTRICTED_ADDRESS_1,
			Some(FlipError::InsufficientLiquidity)
		),
		(
			RedemptionAmount::<FlipBalance>::Exact(RESTRICTED_BALANCE_2),
			RESTRICTED_ADDRESS_2,
			Some(FlipError::InsufficientLiquidity)
		),
	];
	// If the bond is higher than the sum of restrictions, it takes priority over both.
	test_restricted_balances![
		bond_takes_precedence_over_restricted_balances_with_high_bond,
		HIGH_BOND,
		(
			RedemptionAmount::<FlipBalance>::Exact(UNRESTRICTED_BALANCE),
			UNRESTRICTED_ADDRESS,
			Some(FlipError::InsufficientLiquidity)
		),
		(
			RedemptionAmount::<FlipBalance>::Exact(UNRESTRICTED_BALANCE - 50),
			UNRESTRICTED_ADDRESS,
			None::<Error<Test>>
		),
		(
			RedemptionAmount::<FlipBalance>::Exact(RESTRICTED_BALANCE_1),
			RESTRICTED_ADDRESS_1,
			Some(FlipError::InsufficientLiquidity)
		),
		(
			RedemptionAmount::<FlipBalance>::Exact(RESTRICTED_BALANCE_2),
			RESTRICTED_ADDRESS_2,
			Some(FlipError::InsufficientLiquidity)
		),
	];

	#[test]
	fn can_redeem_max_with_only_restricted_funds() {
		new_test_ext().execute_with(|| {
			const RESTRICTED_ADDRESS: EthereumAddress = H160([0x01; 20]);
			const AMOUNT: u128 = 100;
			RestrictedAddresses::<Test>::insert(RESTRICTED_ADDRESS, ());
			assert_ok!(Funding::funded(
				RuntimeOrigin::root(),
				ALICE,
				AMOUNT,
				RESTRICTED_ADDRESS,
				TX_HASH
			));
			assert_ok!(Funding::redeem(
				RuntimeOrigin::signed(ALICE),
				RedemptionAmount::Max,
				RESTRICTED_ADDRESS,
				Default::default()
			));
			assert_eq!(RestrictedBalances::<Test>::get(ALICE).get(&RESTRICTED_ADDRESS), None);
			assert_eq!(Flip::balance(&ALICE), 0);
		});
	}
}

#[test]
fn cannot_redeem_lower_than_redemption_tax() {
	new_test_ext().execute_with(|| {
		const TOTAL_FUNDS: FlipBalance = REDEMPTION_TAX * 10;
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			TOTAL_FUNDS,
			Default::default(),
			Default::default(),
		));

		// Can't withdraw TOTAL_FUNDS otherwise not enough is left to pay the tax.
		assert_noop!(
			Funding::redeem(
				RuntimeOrigin::signed(ALICE),
				RedemptionAmount::Exact(TOTAL_FUNDS - REDEMPTION_TAX + 1),
				Default::default(),
				Default::default()
			),
			crate::Error::<Test>::InsufficientBalance
		);

		// In order to withdraw TOTAL_FUNDS, use RedemptionAmount::Max.
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			RedemptionAmount::Max,
			Default::default(),
			Default::default()
		));
	});
}

#[test]
fn can_bind_redeem_address() {
	new_test_ext().execute_with(|| {
		const REDEEM_ADDRESS: EthereumAddress = H160([0x01; 20]);
		assert_ok!(Funding::bind_redeem_address(RuntimeOrigin::signed(ALICE), REDEEM_ADDRESS));
		assert_event_sequence!(
			Test,
			RuntimeEvent::Funding(crate::Event::BoundRedeemAddress {
				account_id: ALICE,
				address,
			}) if address == REDEEM_ADDRESS,
		);
		assert!(BoundRedeemAddress::<Test>::contains_key(ALICE));
		assert_eq!(BoundRedeemAddress::<Test>::get(ALICE).unwrap(), REDEEM_ADDRESS);
	});
}

#[test]
fn cannot_bind_redeem_address_twice() {
	new_test_ext().execute_with(|| {
		const REDEEM_ADDRESS: EthereumAddress = H160([0x01; 20]);
		assert_ok!(Funding::bind_redeem_address(RuntimeOrigin::signed(ALICE), REDEEM_ADDRESS));
		assert_noop!(
			Funding::bind_redeem_address(RuntimeOrigin::signed(ALICE), REDEEM_ADDRESS),
			crate::Error::<Test>::AccountAlreadyBound
		);
	});
}

#[test]
fn max_redemption_is_net_exact_is_gross() {
	const UNRESTRICTED_AMOUNT: FlipBalance = 100;
	const RESTRICTED_AMOUNT: FlipBalance = 100;
	const TOTAL_BALANCE: FlipBalance = UNRESTRICTED_AMOUNT + RESTRICTED_AMOUNT;
	const UNRESTRICTED_ADDRESS: EthereumAddress = H160([0x01; 20]);
	const RESTRICTED_ADDRESS: EthereumAddress = H160([0x02; 20]);

	#[track_caller]
	fn do_test(
		redemption_address: EthereumAddress,
		redemption_amount: RedemptionAmount<FlipBalance>,
		expected_amount: FlipBalance,
	) {
		new_test_ext().execute_with(|| {
			assert_ok!(Funding::update_restricted_addresses(
				RuntimeOrigin::root(),
				vec![RESTRICTED_ADDRESS],
				Default::default(),
			));
			assert_ok!(Funding::funded(
				RuntimeOrigin::root(),
				ALICE,
				UNRESTRICTED_AMOUNT,
				Default::default(),
				Default::default(),
			));
			assert_ok!(Funding::funded(
				RuntimeOrigin::root(),
				ALICE,
				RESTRICTED_AMOUNT,
				RESTRICTED_ADDRESS,
				Default::default(),
			));
			assert_ok!(Funding::redeem(
				RuntimeOrigin::signed(ALICE),
				redemption_amount,
				redemption_address,
				Default::default(),
			));

			assert!(
				matches!(
					cf_test_utilities::last_event::<Test>(),
					RuntimeEvent::Funding(crate::Event::RedemptionRequested {
						account_id: ALICE,
						amount,
						..
					}) if amount == expected_amount
				),
				"Test failed with redemption_address: {:?}, redemption_amount: {:?}, expected_amount: {:?}. Got: {:#?}",
				redemption_address, redemption_amount, expected_amount, cf_test_utilities::last_event::<Test>()
			);
		});
	}

	// Redeem as many unrestricted funds as possible.
	do_test(UNRESTRICTED_ADDRESS, RedemptionAmount::Max, UNRESTRICTED_AMOUNT - REDEMPTION_TAX);
	// Redeem as many restricted funds as possible.
	do_test(RESTRICTED_ADDRESS, RedemptionAmount::Max, TOTAL_BALANCE - REDEMPTION_TAX);
	// Redeem exact amounts, should be reflected in the event.
	do_test(UNRESTRICTED_ADDRESS, RedemptionAmount::Exact(50), 50);
	do_test(RESTRICTED_ADDRESS, RedemptionAmount::Exact(150), 150);
}

#[test]
fn bond_should_count_toward_restricted_balance() {
	new_test_ext().execute_with(|| {
		const AMOUNT: FlipBalance = 50;
		const RESTRICTED_ADDRESS: EthereumAddress = H160([0x02; 20]);
		// Set restricted addresses.
		assert_ok!(Funding::update_restricted_addresses(
			RuntimeOrigin::root(),
			vec![RESTRICTED_ADDRESS],
			Default::default(),
		));
		// Fund the restricted address.
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			AMOUNT,
			RESTRICTED_ADDRESS,
			Default::default(),
		));
		// Fund an unrestricted address.
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			AMOUNT,
			Default::default(),
			Default::default(),
		));
		// Set the bond.
		Bonder::<Test>::update_bond(&ALICE, AMOUNT);
		// Prof we are setup correctly.
		assert_eq!(Flip::total_balance_of(&ALICE), AMOUNT * 2, "Total balance to be correct.");
		assert_eq!(Flip::bond(&ALICE), AMOUNT, "Bond to be correct.");
		// Assert the restricted balance is correct.
		assert_eq!(
			RestrictedBalances::<Test>::get(ALICE).get(&RESTRICTED_ADDRESS).unwrap(),
			&AMOUNT
		);
		// Redeem the restricted balance.
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			pallet::RedemptionAmount::Exact(AMOUNT / 2),
			RESTRICTED_ADDRESS,
			Default::default()
		));
	});
}

#[test]
fn skip_redemption_of_zero_flip() {
	#[track_caller]
	fn inner_test(funding_amount: FlipBalance, redemption_amount: RedemptionAmount<FlipBalance>) {
		new_test_ext().execute_with(|| {
			assert_ok!(Funding::funded(
				RuntimeOrigin::root(),
				ALICE,
				funding_amount,
				Default::default(),
				Default::default(),
			));
			assert_ok!(Funding::redeem(
				RuntimeOrigin::signed(ALICE),
				redemption_amount,
				Default::default(),
				Default::default()
			));
			assert_event_sequence! {
				Test,
				_,
				RuntimeEvent::Funding(crate::Event::Funded {..}),
				RuntimeEvent::Funding(crate::Event::RedemptionAmountZero {..}),
			};
		});
	}

	inner_test(100, RedemptionAmount::Exact(0));
	inner_test(REDEMPTION_TAX, RedemptionAmount::Max);
}

#[test]
fn check_restricted_balances_are_getting_removed() {
	new_test_ext().execute_with(|| {
		// - Fund account with some restricted balances.
		const AMOUNT: FlipBalance = 50;
		const RESTRICTED_ADDRESS: EthereumAddress = H160([0x02; 20]);
		// Set restricted addresses.
		assert_ok!(Funding::update_restricted_addresses(
			RuntimeOrigin::root(),
			vec![RESTRICTED_ADDRESS],
			Default::default(),
		));
		// Fund the restricted address.
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			AMOUNT,
			RESTRICTED_ADDRESS,
			Default::default(),
		));
		assert!(RestrictedBalances::<Test>::contains_key(ALICE));
		assert_eq!(RestrictedBalances::<Test>::get(ALICE).get(&RESTRICTED_ADDRESS), Some(&AMOUNT));
		assert_ok!(Funding::update_restricted_addresses(
			RuntimeOrigin::root(),
			vec![],
			vec![RESTRICTED_ADDRESS],
		));
		assert!(RestrictedBalances::<Test>::get(ALICE).get(&RESTRICTED_ADDRESS).is_none());
	});
}

#[test]
fn bind_executor_address() {
	new_test_ext().execute_with(|| {
		const EXECUTOR_ADDRESS: EthereumAddress = H160([0x01; 20]);
		assert_ok!(Funding::bind_executor_address(RuntimeOrigin::signed(ALICE), EXECUTOR_ADDRESS));
		assert_event_sequence!(
			Test,
			RuntimeEvent::Funding(crate::Event::BoundExecutorAddress {
				account_id: ALICE,
				address,
			}) if address == EXECUTOR_ADDRESS,
		);
		assert!(BoundExecutorAddress::<Test>::contains_key(ALICE));
		assert_eq!(BoundExecutorAddress::<Test>::get(ALICE).unwrap(), EXECUTOR_ADDRESS);
		assert_noop!(
			Funding::bind_executor_address(RuntimeOrigin::signed(ALICE), EXECUTOR_ADDRESS),
			Error::<Test>::ExecutorAddressAlreadyBound
		);
	});
}

#[test]
fn detect_wrong_executor_address() {
	new_test_ext().execute_with(|| {
		const EXECUTOR_ADDRESS: EthereumAddress = H160([0x01; 20]);
		const WRONG_EXECUTOR_ADDRESS: EthereumAddress = H160([0x02; 20]);
		assert_ok!(Funding::bind_executor_address(RuntimeOrigin::signed(ALICE), EXECUTOR_ADDRESS));
		assert_noop!(
			Funding::redeem(
				RuntimeOrigin::signed(ALICE),
				100.into(),
				ETH_DUMMY_ADDR,
				Some(WRONG_EXECUTOR_ADDRESS)
			),
			Error::<Test>::ExecutorBindingRestrictionViolated
		);
	});
}
