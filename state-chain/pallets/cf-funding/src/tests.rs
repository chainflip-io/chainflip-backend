use crate::{
	mock::*, pallet, ActiveBidder, Error, EthereumAddress, PendingRedemptions, RedemptionAmount,
	RedemptionTax, RestrictedAddresses, RestrictedBalances,
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
use sp_runtime::{traits::BadOrigin, DispatchError};

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

		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			AMOUNT * 2,
			ETH_DUMMY_ADDR,
			TX_HASH
		));

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
fn restricted_funds_getting_recorded() {
	new_test_ext().execute_with(|| {
		const AMOUNT: u128 = 45;
		const RESTRICTED_ADDRESS: EthereumAddress = [0xff; 20];

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
fn restricted_funds_getting_reduced() {
	new_test_ext().execute_with(|| {
		const RESTRICTED_ADDRESS: EthereumAddress = [0x42; 20];
		const UNRESTRICTED_ADDRESS: EthereumAddress = [0x01; 20];
		// Add Address to list of restricted contracts
		RestrictedAddresses::<Test>::insert(RESTRICTED_ADDRESS, ());
		// PendingRedemptions::<Test>::insert(ALICE, ());
		// Add 50 to the restricted address
		assert_ok!(Funding::funded(RuntimeOrigin::root(), ALICE, 50, RESTRICTED_ADDRESS, TX_HASH));
		// and 30 to the unrestricted address
		assert_ok!(Funding::funded(
			RuntimeOrigin::root(),
			ALICE,
			30,
			UNRESTRICTED_ADDRESS,
			TX_HASH
		));
		// Redeem 10
		assert_ok!(Funding::redeem(RuntimeOrigin::signed(ALICE), 10.into(), RESTRICTED_ADDRESS));
		assert_ok!(Funding::redeemed(RuntimeOrigin::root(), ALICE, 10, TX_HASH));
		// Expect the restricted balance to be 70
		assert_eq!(RestrictedBalances::<Test>::get(ALICE).get(&RESTRICTED_ADDRESS).unwrap(), &40);
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
		const VESTING_CONTRACT_1: EthereumAddress = [0x01; 20];
		const VESTING_CONTRACT_2: EthereumAddress = [0x02; 20];
		const UNRESTRICTED_ADDRESS: EthereumAddress = [0x03; 20];
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
			Funding::redeem(RuntimeOrigin::signed(ALICE), 200.into(), UNRESTRICTED_ADDRESS),
			Error::<Test>::AmountToRedeemIsHigherThanRestrictedBalance
		);
		assert_ok!(Funding::redeem(RuntimeOrigin::signed(ALICE), 50.into(), UNRESTRICTED_ADDRESS));
		assert_ok!(Funding::redeemed(RuntimeOrigin::root(), ALICE, 50, TX_HASH));
		// Try to redeem 100 from contract 1
		assert_ok!(Funding::redeem(RuntimeOrigin::signed(ALICE), 100.into(), VESTING_CONTRACT_1));
		assert_ok!(Funding::redeemed(RuntimeOrigin::root(), ALICE, 100, TX_HASH));
		// Try to redeem 400 from contract 2
		assert_ok!(Funding::redeem(RuntimeOrigin::signed(ALICE), 400.into(), VESTING_CONTRACT_2));
		assert_ok!(Funding::redeemed(RuntimeOrigin::root(), ALICE, 400, TX_HASH));
	});
}

#[test]
fn can_withdraw_unrestricted_to_restricted() {
	new_test_ext().execute_with(|| {
		// Contracts
		const RESTRICTED_ADDRESS_1: EthereumAddress = [0x01; 20];
		const RESTRICTED_ADDRESS_2: EthereumAddress = [0x02; 20];
		const UNRESTRICTED_ADDRESS: EthereumAddress = [0x03; 20];
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
			AMOUNT.into(),
			RESTRICTED_ADDRESS_1
		));
	});
}

#[test]
fn can_withdrawal_also_free_funds_to_restricted_address() {
	new_test_ext().execute_with(|| {
		const RESTRICTED_ADDRESS_1: EthereumAddress = [0x01; 20];
		const UNRESTRICTED_ADDRESS: EthereumAddress = [0x03; 20];
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
			(AMOUNT_1 + AMOUNT_2).into(),
			RESTRICTED_ADDRESS_1
		));
		assert_eq!(RestrictedBalances::<Test>::get(ALICE).get(&RESTRICTED_ADDRESS_1).unwrap(), &0);
	});
}

#[cfg(test)]
mod test_restricted_balances {
	use super::*;

	const RESTRICTED_ADDRESS_1: EthereumAddress = [0x01; 20];
	const RESTRICTED_BALANCE_1: u128 = 200;
	const RESTRICTED_ADDRESS_2: EthereumAddress = [0x02; 20];
	const RESTRICTED_BALANCE_2: u128 = 800;
	const UNRESTRICTED_ADDRESS: EthereumAddress = [0x03; 20];
	const UNRESTRICTED_BALANCE: u128 = 100;
	const TOTAL_BALANCE: u128 = RESTRICTED_BALANCE_1 + RESTRICTED_BALANCE_2 + UNRESTRICTED_BALANCE;

	#[track_caller]
	fn run_test(
		redeem_amount: u128,
		redeem_address: EthereumAddress,
		maybe_error: Option<Error<Test>>,
	) {
		new_test_ext().execute_with(|| {
			RestrictedAddresses::<Test>::insert(RESTRICTED_ADDRESS_1, ());
			RestrictedAddresses::<Test>::insert(RESTRICTED_ADDRESS_2, ());

			for (address, amount) in [
				(RESTRICTED_ADDRESS_1, RESTRICTED_BALANCE_1),
				(RESTRICTED_ADDRESS_2, RESTRICTED_BALANCE_2),
				(UNRESTRICTED_ADDRESS, UNRESTRICTED_BALANCE),
			] {
				assert_ok!(Funding::funded(
					RuntimeOrigin::root(),
					ALICE,
					amount,
					address,
					Default::default(),
				));
			}

			let initial_balance = Flip::total_balance_of(&ALICE);
			assert_eq!(initial_balance, 1100);

			match maybe_error {
				None => {
					assert_ok!(Funding::redeem(
						RuntimeOrigin::signed(ALICE),
						redeem_amount.into(),
						redeem_address
					));
					assert!(matches!(
						cf_test_utilities::last_event::<Test>(),
						RuntimeEvent::Funding(crate::Event::RedemptionRequested {
							account_id,
							amount,
							..
						}) if account_id == ALICE && amount == redeem_amount));
					assert_eq!(Flip::total_balance_of(&ALICE), initial_balance - redeem_amount);
				},
				Some(e) => {
					assert_noop!(
						Funding::redeem(
							RuntimeOrigin::signed(ALICE),
							redeem_amount.into(),
							redeem_address
						),
						e,
					);
				},
			}
		});
	}

	macro_rules! test_restricted_balances {
		( $case:ident, $( $spec:expr, )+ ) => {
			#[test]
			fn $case() {
				$(
					std::panic::catch_unwind(||
						run_test(
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
		(MIN_FUNDING, UNRESTRICTED_ADDRESS, None::<Error<Test>>),
		(MIN_FUNDING, RESTRICTED_ADDRESS_1, None::<Error<Test>>),
		(MIN_FUNDING, RESTRICTED_ADDRESS_2, None::<Error<Test>>),
		(UNRESTRICTED_BALANCE, UNRESTRICTED_ADDRESS, None::<Error<Test>>),
		(UNRESTRICTED_BALANCE, RESTRICTED_ADDRESS_1, None::<Error<Test>>),
		(UNRESTRICTED_BALANCE, RESTRICTED_ADDRESS_2, None::<Error<Test>>),
	];
	test_restricted_balances![
		restricted_funds_can_only_be_redeemed_to_restricted_addresses,
		(
			UNRESTRICTED_BALANCE + 1,
			UNRESTRICTED_ADDRESS,
			Some(Error::<Test>::AmountToRedeemIsHigherThanRestrictedBalance)
		),
		(UNRESTRICTED_BALANCE + 1, RESTRICTED_ADDRESS_1, None::<Error<Test>>),
		(UNRESTRICTED_BALANCE + 1, RESTRICTED_ADDRESS_2, None::<Error<Test>>),
		(
			UNRESTRICTED_BALANCE + RESTRICTED_BALANCE_1,
			UNRESTRICTED_ADDRESS,
			Some(Error::<Test>::AmountToRedeemIsHigherThanRestrictedBalance)
		),
		(UNRESTRICTED_BALANCE + RESTRICTED_BALANCE_1, RESTRICTED_ADDRESS_1, None::<Error<Test>>),
		(UNRESTRICTED_BALANCE + RESTRICTED_BALANCE_1, RESTRICTED_ADDRESS_2, None::<Error<Test>>),
	];
	test_restricted_balances![
		higher_than_restricted_amount_1_can_only_be_redeemed_to_restricted_address_2,
		(
			UNRESTRICTED_BALANCE + RESTRICTED_BALANCE_1 + 1,
			UNRESTRICTED_ADDRESS,
			Some(Error::<Test>::AmountToRedeemIsHigherThanRestrictedBalance)
		),
		(
			UNRESTRICTED_BALANCE + RESTRICTED_BALANCE_1 + 1,
			RESTRICTED_ADDRESS_1,
			Some(Error::<Test>::AmountToRedeemIsHigherThanRestrictedBalance)
		),
		(
			UNRESTRICTED_BALANCE + RESTRICTED_BALANCE_1 + 1,
			RESTRICTED_ADDRESS_2,
			None::<Error<Test>>
		),
		(
			UNRESTRICTED_BALANCE + RESTRICTED_BALANCE_2,
			UNRESTRICTED_ADDRESS,
			Some(Error::<Test>::AmountToRedeemIsHigherThanRestrictedBalance)
		),
		(
			UNRESTRICTED_BALANCE + RESTRICTED_BALANCE_2,
			RESTRICTED_ADDRESS_1,
			Some(Error::<Test>::AmountToRedeemIsHigherThanRestrictedBalance)
		),
		(UNRESTRICTED_BALANCE + RESTRICTED_BALANCE_2, RESTRICTED_ADDRESS_2, None::<Error<Test>>),
	];
	test_restricted_balances![
		redemptions_of_more_than_the_higher_restricted_amount_are_not_possible_to_any_address,
		(
			UNRESTRICTED_BALANCE + RESTRICTED_BALANCE_2 + 1,
			UNRESTRICTED_ADDRESS,
			Some(Error::<Test>::AmountToRedeemIsHigherThanRestrictedBalance)
		),
		(
			UNRESTRICTED_BALANCE + RESTRICTED_BALANCE_2 + 1,
			RESTRICTED_ADDRESS_1,
			Some(Error::<Test>::AmountToRedeemIsHigherThanRestrictedBalance)
		),
		(
			UNRESTRICTED_BALANCE + RESTRICTED_BALANCE_2 + 1,
			RESTRICTED_ADDRESS_2,
			Some(Error::<Test>::AmountToRedeemIsHigherThanRestrictedBalance)
		),
		(
			TOTAL_BALANCE,
			UNRESTRICTED_ADDRESS,
			Some(Error::<Test>::AmountToRedeemIsHigherThanRestrictedBalance)
		),
		(
			TOTAL_BALANCE,
			RESTRICTED_ADDRESS_1,
			Some(Error::<Test>::AmountToRedeemIsHigherThanRestrictedBalance)
		),
		(
			TOTAL_BALANCE,
			RESTRICTED_ADDRESS_2,
			Some(Error::<Test>::AmountToRedeemIsHigherThanRestrictedBalance)
		),
	];
}

#[test]
fn cannot_redeem_lower_than_redemption_tax() {
	new_test_ext().execute_with(|| {
		let amount = 1_000;
		assert_ok!(Funding::update_minimum_funding(RuntimeOrigin::root(), amount + 1));
		assert_ok!(Funding::update_redemption_tax(RuntimeOrigin::root(), amount));
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
		assert_ok!(Funding::update_redemption_tax(RuntimeOrigin::root(), amount - 1));

		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(ALICE),
			RedemptionAmount::Exact(amount),
			Default::default()
		));
	});
}
