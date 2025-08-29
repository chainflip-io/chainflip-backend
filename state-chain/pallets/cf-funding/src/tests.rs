// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use crate::{
	mock::*, pallet, BoundExecutorAddress, Error, EthereumAddress, Event, PendingRedemptions,
	RedemptionAmount, RedemptionTax, RestrictedAddresses, RestrictedBalances,
};
use cf_primitives::FlipBalance;
use cf_test_utilities::assert_event_sequence;
use cf_traits::{
	mocks::account_role_registry::MockAccountRoleRegistry, AccountInfo, AccountRoleRegistry,
	Bonding, Chainflip, SetSafeMode, Slashing,
};
use sp_core::H160;

use crate::{BoundRedeemAddress, EthereumDeposit, EthereumDepositAndSCCall};
use cf_traits::SpawnAccount;
use frame_support::{assert_noop, assert_ok, traits::OriginTrait};
use pallet_cf_flip::{Bonder, FlipSlasher};
use sp_runtime::{AccountId32, DispatchError};

const ETH_DUMMY_ADDR: EthereumAddress = H160([42u8; 20]);
const ETH_ZERO_ADDRESS: EthereumAddress = H160([0u8; 20]);
const TX_HASH: pallet::EthTransactionHash = [211u8; 32];

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
		assert_eq!(MockFundingBroadcaster::get_pending_api_calls().len(), 2);

		const TOTAL_A: u128 = AMOUNT_A1 + AMOUNT_A2;
		assert_event_sequence!(
			Test,
			RuntimeEvent::System(frame_system::Event::NewAccount { account: ALICE }),
			RuntimeEvent::Funding(Event::Funded {
				account_id: ALICE,
				tx_hash: TX_HASH,
				funds_added: AMOUNT_A1,
				total_balance: AMOUNT_A1
			}),
			RuntimeEvent::Funding(Event::Funded {
				account_id: ALICE,
				tx_hash: TX_HASH,
				funds_added: AMOUNT_A2,
				total_balance: TOTAL_A,
			}),
			RuntimeEvent::System(frame_system::Event::NewAccount { account: BOB }),
			RuntimeEvent::Funding(Event::Funded {
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
			RuntimeEvent::Funding(Event::Funded {
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
		const REDEEMED_AMOUNT: u128 = FUNDING_AMOUNT;

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
		assert_eq!(MockFundingBroadcaster::get_pending_api_calls().len(), 1);

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
			RuntimeEvent::Funding(Event::Funded {
				account_id: ALICE,
				tx_hash: TX_HASH,
				funds_added: FUNDING_AMOUNT,
				total_balance: FUNDING_AMOUNT
			}),
			RuntimeEvent::Funding(Event::RedemptionRequested {
				account_id: ALICE,
				amount: REDEEMED_AMOUNT,
				broadcast_id: 1,
				expiry_time: 10,
			}),
			RuntimeEvent::System(frame_system::Event::KilledAccount { account: ALICE }),
			RuntimeEvent::Funding(Event::RedemptionSettled(ALICE, REDEEMED_AMOUNT))
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
			Error::<Test>::BondViolation
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
			Error::<Test>::BondViolation,
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
fn can_only_redeem_if_redemption_check_passes() {
	new_test_ext().execute_with(|| {
		const AMOUNT: u128 = 45;

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

		MockRedemptionChecker::set_can_redeem(ALICE, false);

		// Redeem fails of RedemptionCheck fails.
		assert_noop!(
			Funding::redeem(
				RuntimeOrigin::signed(ALICE),
				RedemptionAmount::Max,
				ETH_DUMMY_ADDR,
				Default::default()
			),
			BIDDING_ERR
		);

		// Can redeem now
		MockRedemptionChecker::set_can_redeem(ALICE, true);

		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			(AMOUNT / 2).into(),
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
			RuntimeEvent::Funding(Event::Funded {
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
		// Note that restricted balance is REDEMPTION_TAX less after refund, so adjust the
		// TO_REDEEM accordingly to make sure restricted balance is above minimum
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			(TO_REDEEM - REDEMPTION_TAX).into(),
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

			let (total_funds, restricted_amount) = if redeem_amount == RedemptionAmount::Max {
				(TOTAL_FUNDS, RESTRICTED_AMOUNT)
			} else {
				(TOTAL_FUNDS - REDEMPTION_TAX, RESTRICTED_AMOUNT - REDEMPTION_TAX)
			};

			assert_eq!(
				Flip::total_balance_of(&ALICE),
				total_funds,
				"Expected the full balance, minus redemption tax, to be restored to the account"
			);
			let new_restricted_balance = *RestrictedBalances::<Test>::get(&ALICE)
				.get(&RESTRICTED_ADDRESS)
				.expect("Expected the restricted balance to be restored to the restricted address");
			assert_eq!(
				new_restricted_balance, restricted_amount,
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
	// Redeem significantly less than the restricted balance. Make sure rest is above minimum
	do_test(RedemptionAmount::Exact(RESTRICTED_AMOUNT - REDEMPTION_TAX * 3));
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
		System::assert_last_event(RuntimeEvent::Funding(Event::RedemptionTaxAmountUpdated {
			amount,
		}));
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
		// Redeem using correct redeem and executor should complete successfully
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
	use cf_utilities::assert_matches;
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

					assert_matches!(
						cf_test_utilities::last_event::<Test>(),
						RuntimeEvent::Funding(Event::RedemptionRequested {
							account_id,
							amount,
							..
						}) if account_id == ALICE && amount == expected_redeemed_amount);
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
		restricted_balances_can_only_be_zero_or_above_minimum_after_redeeming,
		NO_BOND,
		// Succeed, with restricted balance1 = 0
		(RedemptionAmount::Exact(RESTRICTED_BALANCE_1), RESTRICTED_ADDRESS_1, None::<Error<Test>>),
		// Succeed, with restricted balance1 = MIN_FUNDING
		(
			RedemptionAmount::Exact(RESTRICTED_BALANCE_1 - MIN_FUNDING - REDEMPTION_TAX),
			RESTRICTED_ADDRESS_1,
			None::<Error<Test>>
		),
		// Fails, because restricted balance1 would have been < MIN_FUNDING
		(
			RedemptionAmount::Exact(RESTRICTED_BALANCE_1 - MIN_FUNDING - REDEMPTION_TAX + 1),
			RESTRICTED_ADDRESS_1,
			Some(Error::<Test>::RestrictedBalanceBelowMinimumFunding)
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
			Some(Error::<Test>::BondViolation)
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
			Some(Error::<Test>::BondViolation)
		),
		(
			RedemptionAmount::<FlipBalance>::Exact(RESTRICTED_BALANCE_2),
			RESTRICTED_ADDRESS_2,
			Some(Error::<Test>::BondViolation)
		),
	];
	// If the bond is higher than the sum of restrictions, it takes priority over both.
	test_restricted_balances![
		bond_takes_precedence_over_restricted_balances_with_high_bond,
		HIGH_BOND,
		(
			RedemptionAmount::<FlipBalance>::Exact(UNRESTRICTED_BALANCE),
			UNRESTRICTED_ADDRESS,
			Some(Error::<Test>::BondViolation)
		),
		(
			RedemptionAmount::<FlipBalance>::Exact(UNRESTRICTED_BALANCE - 50),
			UNRESTRICTED_ADDRESS,
			None::<Error<Test>>
		),
		(
			RedemptionAmount::<FlipBalance>::Exact(RESTRICTED_BALANCE_1),
			RESTRICTED_ADDRESS_1,
			Some(Error::<Test>::BondViolation)
		),
		(
			RedemptionAmount::<FlipBalance>::Exact(RESTRICTED_BALANCE_2),
			RESTRICTED_ADDRESS_2,
			Some(Error::<Test>::BondViolation)
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
			RuntimeEvent::Funding(Event::BoundRedeemAddress {
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
					RuntimeEvent::Funding(Event::RedemptionRequested {
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
	do_test(RESTRICTED_ADDRESS, RedemptionAmount::Max, TOTAL_BALANCE);
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
	new_test_ext().execute_with(|| {
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			100,
			Default::default(),
			Default::default(),
		));
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			RedemptionAmount::Exact(0),
			Default::default(),
			Default::default()
		));
		assert_event_sequence! {
			Test,
			_,
			RuntimeEvent::Funding(Event::Funded {..}),
			RuntimeEvent::Funding(Event::RedemptionAmountZero {..}),
		};
	});
}

#[test]
fn ignore_redemption_tax_when_redeeming_all() {
	new_test_ext().execute_with(|| {
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			100,
			Default::default(),
			Default::default(),
		));
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			RedemptionAmount::Max,
			Default::default(),
			Default::default()
		));
		assert_event_sequence! {
			Test,
			_,
			RuntimeEvent::Funding(Event::Funded {..}),
			RuntimeEvent::Funding(Event::RedemptionRequested { amount: 100, .. }),
		};
	});
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
		assert!(!RestrictedBalances::<Test>::get(ALICE).contains_key(&RESTRICTED_ADDRESS));
	});
}

#[test]
fn bind_executor_address() {
	new_test_ext().execute_with(|| {
		const EXECUTOR_ADDRESS: EthereumAddress = H160([0x01; 20]);
		assert_ok!(Funding::bind_executor_address(RuntimeOrigin::signed(ALICE), EXECUTOR_ADDRESS));
		assert_event_sequence!(
			Test,
			RuntimeEvent::Funding(Event::BoundExecutorAddress {
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

#[test]
fn can_redeem_if_balance_lower_than_restricted_funds() {
	new_test_ext().execute_with(|| {
		const RESTRICTED_ADDRESS_1: EthereumAddress = H160([0x01; 20]);
		const RESTRICTED_ADDRESS_2: EthereumAddress = H160([0x02; 20]);
		const DEBIT_AMOUNT: u128 = 50;
		const RESTRICTED_AMOUNT: u128 = 150;
		const REDEEM_AMOUNT: u128 = 60;
		RestrictedAddresses::<Test>::insert(RESTRICTED_ADDRESS_1, ());
		RestrictedAddresses::<Test>::insert(RESTRICTED_ADDRESS_2, ());
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			RESTRICTED_AMOUNT,
			RESTRICTED_ADDRESS_1,
			TX_HASH
		));
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			RESTRICTED_AMOUNT,
			RESTRICTED_ADDRESS_2,
			TX_HASH
		));

		// we want to have a balance < sum of restricted balances
		FlipSlasher::<Test>::slash_balance(&ALICE, DEBIT_AMOUNT);

		// redemption towards a non restricted address fails
		assert_noop!(
			Funding::redeem(
				RuntimeOrigin::signed(ALICE),
				REDEEM_AMOUNT.into(),
				H160([0x05; 20]),
				Default::default()
			),
			Error::<Test>::InsufficientUnrestrictedFunds
		);
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			REDEEM_AMOUNT.into(),
			RESTRICTED_ADDRESS_1,
			Default::default()
		));
		assert_ok!(Funding::redeemed(RuntimeOrigin::root(), ALICE, REDEEM_AMOUNT, TX_HASH));
		assert_eq!(
			RestrictedBalances::<Test>::get(ALICE).get(&RESTRICTED_ADDRESS_1),
			Some(&(RESTRICTED_AMOUNT - REDEEM_AMOUNT - REDEMPTION_TAX))
		);
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			RedemptionAmount::Max,
			RESTRICTED_ADDRESS_1,
			Default::default()
		));
		assert_ok!(Funding::redeemed(RuntimeOrigin::root(), ALICE, 80, TX_HASH));
		assert_eq!(RestrictedBalances::<Test>::get(ALICE).get(&RESTRICTED_ADDRESS_1), None);
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			RedemptionAmount::Max,
			RESTRICTED_ADDRESS_2,
			Default::default()
		));

		// we are able to withdraw the whole balance
		assert_eq!(Flip::balance(&ALICE), 0);
		// the last restricted address still has some balance in it (sum of initial restricted
		// balances - initial account balance)
		assert_eq!(
			RestrictedBalances::<Test>::get(ALICE).get(&RESTRICTED_ADDRESS_2),
			Some(&(DEBIT_AMOUNT))
		);
	});
}

#[test]
fn cannot_redeem_to_non_restricted_address_with_balance_lower_than_restricted_funds() {
	new_test_ext().execute_with(|| {
		const RESTRICTED_ADDRESS_1: EthereumAddress = H160([0x01; 20]);
		const DEBIT_AMOUNT: u128 = 50;
		const RESTRICTED_AMOUNT: u128 = 150;
		const REDEEM_AMOUNT: u128 = 60;
		RestrictedAddresses::<Test>::insert(RESTRICTED_ADDRESS_1, ());
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			RESTRICTED_AMOUNT,
			RESTRICTED_ADDRESS_1,
			TX_HASH
		));

		// we want to have a balance < sum of restricted balances
		FlipSlasher::<Test>::slash_balance(&ALICE, DEBIT_AMOUNT);

		assert_noop!(
			Funding::redeem(
				RuntimeOrigin::signed(ALICE),
				REDEEM_AMOUNT.into(),
				H160([0x05; 20]),
				Default::default()
			),
			Error::<Test>::InsufficientUnrestrictedFunds
		);
	});
}

#[test]
fn account_references_must_be_zero_for_full_redeem() {
	const FUNDING_AMOUNT: FlipBalance = 100;
	new_test_ext().execute_with(|| {
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			FUNDING_AMOUNT,
			Default::default(),
			Default::default()
		));
		assert_eq!(
			frame_system::Pallet::<Test>::providers(&ALICE),
			1,
			"Funding pallet should increment the provider count on account creation."
		);

		<<Test as Chainflip>::AccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(&ALICE).unwrap();

		assert_noop!(
			Funding::redeem(
				OriginTrait::signed(ALICE),
				RedemptionAmount::Max,
				Default::default(),
				Default::default()
			),
			Error::<Test>::AccountMustBeUnregistered,
		);

		<<Test as Chainflip>::AccountRoleRegistry as AccountRoleRegistry<Test>>::deregister_as_validator(&ALICE).unwrap();

		assert_ok!(Funding::redeem(
			OriginTrait::signed(ALICE),
			RedemptionAmount::Max,
			Default::default(),
			Default::default()
		),);

		assert_ok!(Funding::redeemed(
			RuntimeOrigin::root(),
			ALICE,
			FUNDING_AMOUNT,
			Default::default()
		),);

		assert_eq!(
			frame_system::Pallet::<Test>::providers(&ALICE),
			0,
			"Funding pallet should decrement the provider count on final redemption."
		);
	});
}

#[test]
fn only_governance_can_update_settings() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Funding::update_minimum_funding(RuntimeOrigin::signed(ALICE), 0),
			sp_runtime::traits::BadOrigin,
		);
		assert_noop!(
			Funding::update_restricted_addresses(RuntimeOrigin::signed(ALICE), vec![], vec![]),
			sp_runtime::traits::BadOrigin,
		);
		assert_noop!(
			Funding::update_redemption_tax(RuntimeOrigin::signed(ALICE), 0),
			sp_runtime::traits::BadOrigin,
		);
	});
}

pub mod ethereum_sc_calls {
	use super::*;

	const CALLER: EthereumAddress = H160([7u8; 20]);
	const CALLER_32: AccountId32 = AccountId32::new([3u8; 32]);
	const FUND_AMOUNT: u128 = 1000u128;
	const VALID_CALL_BYTES: [u8; 1] = [0];
	const INVALID_CALL_BYTES: [u8; 2] = [12, 3];

	#[test]
	fn execute_sc_call_successfully() {
		new_test_ext().execute_with(|| {
			assert_ok!(Funding::execute_sc_call(
				RuntimeOrigin::root(),
				EthereumDepositAndSCCall {
					deposit: EthereumDeposit::FlipToSCGateway { amount: FUND_AMOUNT },
					call: VALID_CALL_BYTES.to_vec()
				},
				CALLER,
				CALLER_32,
				TX_HASH
			));
			assert_event_sequence!(
				Test,
				RuntimeEvent::System(frame_system::Event::NewAccount { account: CALLER_32 }),
				RuntimeEvent::Funding(Event::Funded {
					account_id: CALLER_32,
					tx_hash: TX_HASH,
					funds_added: FUND_AMOUNT,
					total_balance: FUND_AMOUNT,
				}),
				RuntimeEvent::Funding(Event::SCCallExecuted {
					caller: CALLER_32,
					sc_call: _,
					eth_tx_hash: TX_HASH
				})
			);
		});
	}

	#[test]
	fn cannot_decode_sc_call() {
		new_test_ext().execute_with(|| {
			assert_ok!(Funding::execute_sc_call(
				RuntimeOrigin::root(),
				EthereumDepositAndSCCall {
					deposit: EthereumDeposit::FlipToSCGateway { amount: FUND_AMOUNT },
					call: INVALID_CALL_BYTES.to_vec()
				},
				CALLER,
				CALLER_32,
				TX_HASH
			));
			assert_event_sequence!(
				Test,
				RuntimeEvent::System(frame_system::Event::NewAccount { account: CALLER_32 }),
				RuntimeEvent::Funding(Event::Funded {
					account_id: CALLER_32,
					tx_hash: TX_HASH,
					funds_added: FUND_AMOUNT,
					total_balance: FUND_AMOUNT,
				}),
				RuntimeEvent::Funding(Event::SCCallCannotBeDecoded {
					caller: CALLER_32,
					sc_call_bytes: _,
					eth_tx_hash: TX_HASH
				})
			);
		});
	}

	#[test]
	fn no_deposit_only_call() {
		new_test_ext().execute_with(|| {
			assert_ok!(Funding::execute_sc_call(
				RuntimeOrigin::root(),
				EthereumDepositAndSCCall {
					deposit: EthereumDeposit::NoDeposit,
					call: VALID_CALL_BYTES.to_vec()
				},
				CALLER,
				CALLER_32,
				TX_HASH
			));
			assert_event_sequence!(
				Test,
				RuntimeEvent::Funding(Event::SCCallExecuted {
					caller: CALLER_32,
					sc_call: _,
					eth_tx_hash: TX_HASH
				})
			);
		});
	}
}
mod utils {
	use super::*;
	use cf_primitives::AccountRole;

	#[derive(Debug, Clone)]
	pub struct AccountSetup {
		account: AccountId32,
		initial_balance: u128,
		funding_address: Option<EthereumAddress>,
		deposits: Vec<(u128, EthereumAddress)>,
		bound_redeem_address: Option<EthereumAddress>,
		bound_executor_address: Option<EthereumAddress>,
		bond: Option<u128>,
		role: Option<AccountRole>,
		can_redeem: bool,
	}

	impl AccountSetup {
		pub fn new(account: AccountId32) -> Self {
			Self {
				account,
				// default is not zero because an account with zero balance can't exist.
				initial_balance: MIN_FUNDING,
				funding_address: None,
				deposits: Vec::new(),
				bound_redeem_address: None,
				bound_executor_address: None,
				bond: None,
				role: None,
				can_redeem: true,
			}
		}

		pub fn account(&self) -> AccountId32 {
			self.account.clone()
		}

		pub fn with_balance(
			mut self,
			initial_balance: u128,
			funding_address: Option<EthereumAddress>,
		) -> Self {
			self.initial_balance = initial_balance;
			self.funding_address = funding_address;
			self
		}

		pub fn with_bound_redeem_address(mut self, address: EthereumAddress) -> Self {
			self.bound_redeem_address = Some(address);
			self
		}

		pub fn with_bound_executor_address(mut self, address: EthereumAddress) -> Self {
			self.bound_executor_address = Some(address);
			self
		}

		pub fn with_bond(mut self, bond: u128) -> Self {
			self.bond = Some(bond);
			self
		}

		pub fn with_role(mut self, role: AccountRole) -> Self {
			self.role = Some(role);
			self
		}

		pub fn with_validator_role(self) -> Self {
			self.with_role(AccountRole::Validator)
		}

		pub fn with_can_redeem(mut self, can_redeem: bool) -> Self {
			self.can_redeem = can_redeem;
			self
		}
	}

	pub fn setup_test(
		accounts: Vec<AccountSetup>,
		restricted_addresses: Vec<EthereumAddress>,
	) -> Result<(), sp_runtime::DispatchError> {
		// Set up restricted addresses
		assert_ok!(Funding::update_restricted_addresses(
			RuntimeOrigin::root(),
			restricted_addresses,
			vec![],
		));

		// Set up accounts
		for setup in accounts {
			if let Some(role) = setup.role {
				<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_account_role(
					&setup.account,
					role,
				)?;
			}

			MockRedemptionChecker::set_can_redeem(setup.account(), setup.can_redeem);

			if setup.initial_balance > 0 {
				let funding_address = setup.funding_address.unwrap_or_default();
				Funding::funded(
					RuntimeOrigin::root(),
					setup.account(),
					setup.initial_balance,
					funding_address,
					TX_HASH,
				)?;
			} else {
				panic!("Account setup requires a non-zero initial balance.");
			}

			for (amount, address) in setup.deposits.clone().into_iter().rev() {
				Funding::funded(RuntimeOrigin::root(), setup.account(), amount, address, TX_HASH)?;
			}

			if let Some(address) = setup.bound_redeem_address {
				Funding::bind_redeem_address(RuntimeOrigin::signed(setup.account()), address)?;
			}

			if let Some(address) = setup.bound_executor_address {
				Funding::bind_executor_address(RuntimeOrigin::signed(setup.account()), address)?;
			}

			if let Some(bond) = setup.bond {
				Bonder::<Test>::update_bond(&setup.account, bond);
			}
		}

		Ok(())
	}
}

pub mod rebalancing {
	use super::{utils::*, *};

	#[test]
	fn rebalance_unrestricted_funds() {
		new_test_ext().execute_with(|| {
			const AMOUNT: u128 = 100;
			const UNRESTRICTED_ADDRESS: EthereumAddress = H160([0x01; 20]);

			assert_ok!(setup_test(
				vec![
					AccountSetup::new(ALICE).with_balance(AMOUNT, Some(UNRESTRICTED_ADDRESS)),
					AccountSetup::new(BOB),
				],
				vec![]
			));

			// ALICE funds BOB with 100 unrestricted FLIP.
			assert_ok!(Funding::rebalance(
				OriginTrait::signed(ALICE),
				BOB,
				Some(UNRESTRICTED_ADDRESS),
				AMOUNT.into()
			));
			assert_event_sequence!(
				Test,
				RuntimeEvent::System(frame_system::Event::NewAccount { account: ALICE }),
				RuntimeEvent::Funding(Event::Funded {
					account_id: ALICE,
					tx_hash: _,
					funds_added: AMOUNT,
					total_balance: AMOUNT
				}),
				RuntimeEvent::System(frame_system::Event::NewAccount { account: BOB }),
				RuntimeEvent::Funding(Event::Funded {
					account_id: BOB,
					tx_hash: _,
					funds_added: MIN_FUNDING,
					total_balance: MIN_FUNDING
				}),
				RuntimeEvent::Funding(Event::Rebalance {
					source_account_id: ALICE,
					recipient_account_id: BOB,
					amount: AMOUNT,
				}),
			);
			assert_eq!(
				Flip::total_balance_of(&BOB),
				AMOUNT + MIN_FUNDING,
				"Total balance to be correct."
			);
		});
	}

	#[test]
	fn rebalance_restricted_funds() {
		new_test_ext().execute_with(|| {
			const AMOUNT: u128 = 100;
			const RESTRICTED_ADDRESS: EthereumAddress = H160([0x01; 20]);

			assert_ok!(setup_test(
				vec![
					AccountSetup::new(ALICE).with_balance(AMOUNT, Some(RESTRICTED_ADDRESS)),
					AccountSetup::new(BOB),
				],
				vec![RESTRICTED_ADDRESS]
			));

			// ALICE funds BOB with 100 restricted FLIP.
			assert_ok!(Funding::rebalance(
				OriginTrait::signed(ALICE),
				BOB,
				Some(RESTRICTED_ADDRESS),
				AMOUNT.into()
			));
			assert_eq!(RestrictedBalances::<Test>::get(ALICE).get(&RESTRICTED_ADDRESS), None);
			assert_eq!(
				RestrictedBalances::<Test>::get(BOB).get(&RESTRICTED_ADDRESS),
				Some(&AMOUNT)
			);
		});
	}

	#[test]
	fn rebalance_only_a_apart_of_the_restricted_funds() {
		new_test_ext().execute_with(|| {
			const AMOUNT: u128 = 100;
			const AMOUNT_MOVE: u128 = AMOUNT / 2;
			const RESTRICTED_ADDRESS: EthereumAddress = H160([0x01; 20]);

			assert_ok!(setup_test(
				vec![
					AccountSetup::new(ALICE)
						.with_validator_role()
						.with_balance(AMOUNT, Some(RESTRICTED_ADDRESS)),
					AccountSetup::new(BOB).with_validator_role(),
				],
				vec![RESTRICTED_ADDRESS]
			));

			assert_ok!(Funding::rebalance(
				OriginTrait::signed(ALICE),
				BOB,
				Some(RESTRICTED_ADDRESS),
				RedemptionAmount::Exact(AMOUNT_MOVE)
			));

			// We burn the redemption tax from the restricted balance of the sender.
			assert_eq!(
				*RestrictedBalances::<Test>::get(BOB).get(&RESTRICTED_ADDRESS).unwrap(),
				AMOUNT_MOVE
			);

			assert_eq!(
				*RestrictedBalances::<Test>::get(ALICE).get(&RESTRICTED_ADDRESS).unwrap(),
				AMOUNT_MOVE
			);
		});
	}

	#[test]
	fn ensure_bound_address_restrictions_enforced_during_rebalance() {
		new_test_ext().execute_with(|| {
			const AMOUNT: u128 = 100;
			const ADDRESS_A: EthereumAddress = H160([0x01; 20]);
			const ADDRESS_B: EthereumAddress = H160([0x02; 20]);

			assert_ok!(setup_test(
				vec![
					AccountSetup::new(ALICE)
						.with_balance(AMOUNT, None)
						.with_bound_redeem_address(ADDRESS_A),
					AccountSetup::new(BOB)
						.with_balance(AMOUNT, None)
						.with_bound_redeem_address(ADDRESS_B),
					AccountSetup::new(CHARLIE).with_balance(AMOUNT, None),
				],
				vec![]
			));

			// Case: Rebalance to account with different bound address.
			assert_noop!(
				Funding::rebalance(OriginTrait::signed(ALICE), BOB, None, MIN_FUNDING.into()),
				Error::<Test>::AccountBindingRestrictionViolated
			);
			// Case: Rebalance to account with no bound address.
			assert_noop!(
				Funding::rebalance(OriginTrait::signed(ALICE), CHARLIE, None, MIN_FUNDING.into()),
				Error::<Test>::AccountBindingRestrictionViolated
			);
			// Case: Rebalance from account with no bound address to account with bound address.
			assert_noop!(
				Funding::rebalance(OriginTrait::signed(CHARLIE), ALICE, None, MIN_FUNDING.into()),
				Error::<Test>::AccountBindingRestrictionViolated
			);
		});
	}

	#[test]
	fn rebalancing_amount_must_be_above_min_funding() {
		new_test_ext().execute_with(|| {
			const AMOUNT: u128 = 100;

			assert_ok!(setup_test(
				vec![AccountSetup::new(ALICE).with_balance(AMOUNT, None), AccountSetup::new(BOB),],
				vec![]
			));

			// Try to rebalance with an amount below the minimum funding.
			assert_noop!(
				Funding::rebalance(OriginTrait::signed(ALICE), BOB, None, (MIN_FUNDING - 1).into()),
				Error::<Test>::MinimumRebalanceAmount
			);
			// Rebalance with the minimum funding amount.
			assert_ok!(Funding::rebalance(
				OriginTrait::signed(ALICE),
				BOB,
				None,
				MIN_FUNDING.into()
			));
		});
	}

	#[test]
	fn fund_rebalance_and_redeem_does_not_allow_unauthorized_redemptions() {
		new_test_ext().execute_with(|| {
			const AMOUNT: u128 = 100;
			const REBALANCE_AMOUNT: u128 = 50;
			const RESTRICTED_ADDRESS: EthereumAddress = H160([0x01; 20]);
			const UNRESTRICTED_ADDRESS: EthereumAddress = H160([0x02; 20]);

			assert_ok!(setup_test(
				vec![
					AccountSetup::new(ALICE)
						.with_validator_role()
						.with_balance(AMOUNT, Some(RESTRICTED_ADDRESS)),
					AccountSetup::new(BOB),
				],
				vec![RESTRICTED_ADDRESS]
			));

			// Try to rebalance to BOBs liquid funds.
			assert_noop!(
				Funding::rebalance(OriginTrait::signed(ALICE), BOB, None, AMOUNT.into()),
				Error::<Test>::InsufficientUnrestrictedFunds
			);

			// Try to rebalance to BOB under an unrestricted address.
			assert_noop!(
				Funding::rebalance(
					OriginTrait::signed(ALICE),
					BOB,
					Some(UNRESTRICTED_ADDRESS),
					AMOUNT.into()
				),
				Error::<Test>::InsufficientUnrestrictedFunds
			);

			// Rebalance to BOB under a restricted address.
			assert_ok!(Funding::rebalance(
				OriginTrait::signed(ALICE),
				BOB,
				Some(RESTRICTED_ADDRESS),
				REBALANCE_AMOUNT.into()
			));

			// Try to redeem some funds to an unrestricted address.
			assert_noop!(
				Funding::redeem(
					OriginTrait::signed(BOB),
					REBALANCE_AMOUNT.into(),
					UNRESTRICTED_ADDRESS,
					Default::default()
				),
				Error::<Test>::InsufficientUnrestrictedFunds
			);

			// Try to rebalance to ALICE liquid funds.
			assert_noop!(
				Funding::rebalance(OriginTrait::signed(BOB), ALICE, None, REBALANCE_AMOUNT.into()),
				Error::<Test>::InsufficientUnrestrictedFunds
			);

			// Try to rebalance to ALICE under an unrestricted address.
			assert_noop!(
				Funding::rebalance(
					OriginTrait::signed(BOB),
					ALICE,
					Some(UNRESTRICTED_ADDRESS),
					REBALANCE_AMOUNT.into()
				),
				Error::<Test>::InsufficientUnrestrictedFunds
			);

			// Rebalance back to ALICE under a restricted address.
			assert_ok!(Funding::rebalance(
				OriginTrait::signed(BOB),
				ALICE,
				Some(RESTRICTED_ADDRESS),
				REBALANCE_AMOUNT.into()
			));

			assert_ok!(
				<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::deregister_as_validator(
					&ALICE
				)
			);

			assert_eq!(Flip::total_balance_of(&BOB), MIN_FUNDING);
			assert_eq!(Flip::total_balance_of(&ALICE), AMOUNT);

			// Redeem successfully all funds to BOB to the restricted address.
			assert_ok!(Funding::redeem(
				OriginTrait::signed(BOB),
				RedemptionAmount::Max,
				RESTRICTED_ADDRESS,
				Default::default()
			));

			// Redeem successfully all funds to ALICE to the restricted address.
			assert_ok!(Funding::redeem(
				OriginTrait::signed(ALICE),
				RedemptionAmount::Max,
				RESTRICTED_ADDRESS,
				Default::default()
			));

			assert_eq!(Flip::total_balance_of(&BOB), 0);
			assert_eq!(Flip::total_balance_of(&ALICE), 0);

			assert!(PendingRedemptions::<Test>::get(BOB).is_some());
			assert!(PendingRedemptions::<Test>::get(ALICE).is_some());

			let mut api_calls = MockFundingBroadcaster::get_pending_api_calls();
			assert_eq!(api_calls.len(), 2);

			let api_call_a = api_calls.pop().unwrap();
			let api_call_b = api_calls.pop().unwrap();

			let successful_redemption_amount_a = api_call_a.amount;
			let successful_redemption_amount_b = api_call_b.amount;

			// Note: No tax is taken during the redemption since we redeem all.
			assert_eq!(successful_redemption_amount_a, AMOUNT);
			assert_eq!(successful_redemption_amount_b, MIN_FUNDING);

			// Proof balance integrity of all operations.
			assert_eq!(
				AMOUNT + MIN_FUNDING, // MIN_FUNDING is the initial balance of BOB
				Flip::total_balance_of(&ALICE) +
					Flip::total_balance_of(&BOB) +
					successful_redemption_amount_a +
					successful_redemption_amount_b
			);
		});
	}

	#[test]
	fn rebalance_during_redemption_does_not_lead_to_double_spending() {
		new_test_ext().execute_with(|| {
			const AMOUNT: u128 = 100;
			const AMOUNT_TO_REDEEM: u128 = 50;
			const REBALANCE_AMOUNT_TO_HIGH: u128 = 60;
			const REBALANCE_AMOUNT: u128 = 30;
			const TAX: u128 = 5;
			const UNRESTRICTED_ADDRESS: EthereumAddress = H160([0x01; 20]);

			assert_ok!(setup_test(
				vec![
					AccountSetup::new(ALICE)
						.with_validator_role()
						.with_balance(AMOUNT, Some(UNRESTRICTED_ADDRESS)),
					AccountSetup::new(BOB).with_validator_role(),
				],
				vec![]
			));

			assert_ok!(Funding::redeem(
				OriginTrait::signed(ALICE),
				RedemptionAmount::Exact(AMOUNT_TO_REDEEM),
				UNRESTRICTED_ADDRESS,
				Default::default()
			));

			assert_noop!(
				Funding::rebalance(
					OriginTrait::signed(ALICE),
					BOB,
					Some(UNRESTRICTED_ADDRESS),
					REBALANCE_AMOUNT_TO_HIGH.into()
				),
				Error::<Test>::InsufficientBalance
			);

			assert_ok!(Funding::rebalance(
				OriginTrait::signed(ALICE),
				BOB,
				Some(UNRESTRICTED_ADDRESS),
				REBALANCE_AMOUNT.into()
			));

			let on_chain_balance_alice = Flip::total_balance_of(&ALICE);
			let on_chain_balance_bob = Flip::total_balance_of(&BOB);

			assert_eq!(on_chain_balance_alice, AMOUNT - AMOUNT_TO_REDEEM - REBALANCE_AMOUNT - TAX);
			assert_eq!(on_chain_balance_bob, MIN_FUNDING + REBALANCE_AMOUNT);

			assert!(PendingRedemptions::<Test>::get(ALICE).is_some());

			let mut api_calls = MockFundingBroadcaster::get_pending_api_calls();
			assert_eq!(api_calls.len(), 1);

			let api_call_1 = api_calls.pop().unwrap();

			let successful_redemption_amount = api_call_1.amount;

			assert_eq!(successful_redemption_amount, AMOUNT_TO_REDEEM);

			assert_eq!(
				AMOUNT + MIN_FUNDING, // MIN_FUNDING is the initial balance of BOB
				on_chain_balance_alice + on_chain_balance_bob + successful_redemption_amount + TAX
			);
		});
	}

	#[test]
	fn rebalance_to_non_bidding_validator_fails() {
		new_test_ext().execute_with(|| {
			const AMOUNT: u128 = 100;
			const REBALANCE_AMOUNT: u128 = 30;
			const UNRESTRICTED_ADDRESS: EthereumAddress = H160([0x01; 20]);

			assert_ok!(setup_test(
				vec![
					AccountSetup::new(ALICE)
						.with_validator_role()
						.with_balance(AMOUNT, Some(UNRESTRICTED_ADDRESS))
						.with_can_redeem(false),
					AccountSetup::new(BOB).with_validator_role(),
				],
				vec![]
			));

			// ALICE is bidding, BOB not, so rebalance should fail
			assert_noop!(
				Funding::rebalance(
					OriginTrait::signed(ALICE),
					BOB,
					Some(UNRESTRICTED_ADDRESS),
					REBALANCE_AMOUNT.into()
				),
				Error::<Test>::CanNotRebalanceToNotBiddingValidator
			);

			MockRedemptionChecker::set_can_redeem(BOB, false);

			// IF ALICE is bidding, and BOB is as well, rebalance should succeed
			assert_ok!(Funding::rebalance(
				OriginTrait::signed(ALICE),
				BOB,
				Some(UNRESTRICTED_ADDRESS),
				REBALANCE_AMOUNT.into()
			));
		});
	}

	#[test]
	fn ensure_bound_address_restriction_is_enforced_during_rebalance() {
		new_test_ext().execute_with(|| {
			const AMOUNT: u128 = 100;
			const ADDRESS_A: EthereumAddress = H160([0x01; 20]);
			const ADDRESS_B: EthereumAddress = H160([0x02; 20]);

			assert_ok!(setup_test(
				vec![
					AccountSetup::new(ALICE).with_bound_redeem_address(ADDRESS_A),
					AccountSetup::new(BOB),
				],
				vec![]
			));

			assert_noop!(
				Funding::rebalance(OriginTrait::signed(ALICE), BOB, None, AMOUNT.into()),
				Error::<Test>::AccountBindingRestrictionViolated
			);

			// Update BOB's bound address
			assert_ok!(Funding::bind_redeem_address(RuntimeOrigin::signed(BOB), ADDRESS_B));

			assert_noop!(
				Funding::rebalance(OriginTrait::signed(ALICE), BOB, None, AMOUNT.into()),
				Error::<Test>::AccountBindingRestrictionViolated
			);

			BoundRedeemAddress::<Test>::insert(BOB, ADDRESS_A);

			assert_noop!(
				Funding::rebalance(OriginTrait::signed(ALICE), BOB, None, AMOUNT.into()),
				Error::<Test>::InsufficientBalance
			);
		});
	}

	#[test]
	fn ensure_executor_address_restriction_is_enforced_during_rebalance() {
		new_test_ext().execute_with(|| {
			const AMOUNT: u128 = 100;
			const ADDRESS_A: EthereumAddress = H160([0x01; 20]);
			const ADDRESS_B: EthereumAddress = H160([0x02; 20]);

			assert_ok!(setup_test(
				vec![
					AccountSetup::new(ALICE).with_bound_executor_address(ADDRESS_A),
					AccountSetup::new(BOB),
				],
				vec![]
			));

			assert_noop!(
				Funding::rebalance(OriginTrait::signed(ALICE), BOB, None, AMOUNT.into()),
				Error::<Test>::ExecutorBindingRestrictionViolated
			);

			// Update BOB's bound executor address
			assert_ok!(Funding::bind_executor_address(RuntimeOrigin::signed(BOB), ADDRESS_B));

			assert_noop!(
				Funding::rebalance(OriginTrait::signed(ALICE), BOB, None, AMOUNT.into()),
				Error::<Test>::ExecutorBindingRestrictionViolated
			);

			BoundExecutorAddress::<Test>::insert(BOB, ADDRESS_A);

			assert_noop!(
				Funding::rebalance(OriginTrait::signed(ALICE), BOB, None, AMOUNT.into()),
				Error::<Test>::InsufficientBalance
			);
		});
	}

	#[test]
	fn cannot_rebalance_illiquid_funds() {
		new_test_ext().execute_with(|| {
			const AMOUNT: u128 = 100;
			const RESTRICTED_ADDRESS: EthereumAddress = H160([0x01; 20]);

			assert_ok!(setup_test(
				vec![
					AccountSetup::new(ALICE)
						.with_balance(AMOUNT, Some(RESTRICTED_ADDRESS))
						.with_bond(AMOUNT),
					AccountSetup::new(BOB),
				],
				vec![]
			));

			assert_noop!(
				Funding::rebalance(OriginTrait::signed(ALICE), BOB, None, AMOUNT.into()),
				Error::<Test>::BondViolation,
			);
		});
	}
}

pub mod sub_accounts {
	use super::{utils::*, *};
	use crate::MinimumFunding;
	use sp_runtime::traits::Zero;

	#[test]
	fn cannot_spawn_account_if_parent_account_is_bidding() {
		new_test_ext().execute_with(|| {
			MockRedemptionChecker::set_can_redeem(ALICE, false);
			assert_noop!(
				Funding::spawn_sub_account(&ALICE, 0, MinimumFunding::<Test>::get()),
				Error::<Test>::CannotSpawnDuringAuctionPhase,
			);
		});
	}

	#[test]
	fn can_spawn_sub_account_and_fund_it_via_rebalance() {
		new_test_ext().execute_with(|| {
			const AMOUNT: u128 = 100;
			const INITIAL_BALANCE: u128 = 10;
			assert_ok!(setup_test(
				vec![AccountSetup::new(ALICE).with_balance(AMOUNT, None)],
				vec![]
			));
			let sub_account_id = Funding::spawn_sub_account(&ALICE, 0, INITIAL_BALANCE).unwrap();

			assert_eq!(Flip::total_balance_of(&ALICE), AMOUNT - INITIAL_BALANCE);
			assert_eq!(Flip::total_balance_of(&sub_account_id), INITIAL_BALANCE);
		});
	}

	#[test]
	fn cannot_spawn_sub_account_twice() {
		new_test_ext().execute_with(|| {
			const AMOUNT: u128 = 100;
			const SUB_ACCT_INDEX: u8 = 0;
			assert_ok!(setup_test(
				vec![AccountSetup::new(ALICE).with_balance(AMOUNT, None)],
				vec![]
			));
			Funding::spawn_sub_account(&ALICE, SUB_ACCT_INDEX, MinimumFunding::<Test>::get())
				.unwrap();
			assert_noop!(
				Funding::spawn_sub_account(&ALICE, SUB_ACCT_INDEX, MinimumFunding::<Test>::get()),
				Error::<Test>::AccountAlreadyExists,
			);
		});
	}

	#[test]
	fn restrictions_are_getting_inherited_to_sub_accounts() {
		new_test_ext().execute_with(|| {
			const AMOUNT: u128 = 100;
			const SUB_ACCT_INDEX: u8 = 0;

			const RESTRICTED_ADDRESS_A: EthereumAddress = H160([0x01; 20]);
			const RESTRICTED_ADDRESS_B: EthereumAddress = H160([0x02; 20]);

			BoundRedeemAddress::<Test>::insert(ALICE, RESTRICTED_ADDRESS_A);
			BoundExecutorAddress::<Test>::insert(ALICE, RESTRICTED_ADDRESS_B);

			assert_ok!(setup_test(
				vec![AccountSetup::new(ALICE).with_balance(AMOUNT, None)],
				vec![]
			));

			let sub_account_id =
				Funding::spawn_sub_account(&ALICE, SUB_ACCT_INDEX, MinimumFunding::<Test>::get())
					.unwrap();

			assert_eq!(
				BoundRedeemAddress::<Test>::get(&sub_account_id),
				Some(RESTRICTED_ADDRESS_A)
			);
			assert_eq!(
				BoundExecutorAddress::<Test>::get(&sub_account_id),
				Some(RESTRICTED_ADDRESS_B)
			);
		});
	}

	#[test]
	fn cannot_remove_parent_account_before_sub_accounts() {
		new_test_ext().execute_with(|| {
			const AMOUNT: u128 = 100;
			const SUB_ACCT_INDEX: u8 = 0;
			assert_ok!(setup_test(
				vec![AccountSetup::new(ALICE).with_balance(AMOUNT, None)],
				vec![]
			));

			let sub_account_id =
				Funding::spawn_sub_account(&ALICE, SUB_ACCT_INDEX, MinimumFunding::<Test>::get())
					.unwrap();

			// Try to redeem all funds from parent account, which should fail
			// because the parent account has remaining consumers (sub-accounts)
			assert_noop!(
				Funding::redeem(
					RuntimeOrigin::signed(ALICE),
					RedemptionAmount::Max,
					H160::from([1; 20]),
					None
				),
				Error::<Test>::AccountHasRemainingConsumers
			);

			// Verify that parent account still exists and has the expected balance
			assert!(!Flip::balance(&ALICE).is_zero());
			assert!(frame_system::Pallet::<Test>::account_exists(&ALICE));
			assert!(frame_system::Pallet::<Test>::account_exists(&sub_account_id));

			// Move funds from the sub-account back to the parent account, removing the sub-account
			// in the process.
			assert_ok!(Funding::rebalance(
				OriginTrait::signed(sub_account_id.clone()),
				ALICE,
				None,
				RedemptionAmount::Max
			));
			assert!(!frame_system::Pallet::<Test>::account_exists(&sub_account_id));
			assert_eq!(Flip::balance(&ALICE), AMOUNT);

			// Now redeem all funds from the parent account, which should succeed.
			assert_ok!(Funding::redeem(
				RuntimeOrigin::signed(ALICE),
				RedemptionAmount::Max,
				H160::from([1; 20]),
				None
			));
		});
	}

	#[test]
	fn cannot_spawn_sub_account_from_sub_account() {
		new_test_ext().execute_with(|| {
			const AMOUNT: u128 = 100;
			const SUB_ACCT_INDEX: u8 = 0;
			assert_ok!(setup_test(
				vec![AccountSetup::new(ALICE).with_balance(AMOUNT, None)],
				vec![]
			));

			// First, spawn a sub-account from the parent account
			let sub_account_id =
				Funding::spawn_sub_account(&ALICE, SUB_ACCT_INDEX, MinimumFunding::<Test>::get())
					.unwrap();

			// Now try to spawn a sub-account from the sub-account, which should fail
			assert_noop!(
				Funding::spawn_sub_account(
					&sub_account_id,
					SUB_ACCT_INDEX,
					MinimumFunding::<Test>::get()
				),
				Error::<Test>::CannotSpawnFromSubAccount
			);

			// Verify that the original sub-account still exists
			assert!(frame_system::Pallet::<Test>::account_exists(&sub_account_id));
		});
	}
}
