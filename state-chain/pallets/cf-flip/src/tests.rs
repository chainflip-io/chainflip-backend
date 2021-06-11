use std::mem;

use crate::{Config, Error, OffchainFunds, TotalIssuance, mock::*};
use frame_support::traits::Imbalance;
use cf_traits::{Emissions, StakeTransfer};
use frame_support::{assert_noop, assert_ok};


#[test]
fn account_to_account() {
	new_test_ext().execute_with(|| {
		// Account to account
		Flip::settle(&ALICE, Flip::debit(&BOB, 1).into());
		assert_eq!(Flip::total_balance_of(&ALICE), 101);
		assert_eq!(Flip::total_balance_of(&BOB), 49);
		check_balance_integrity();

		Flip::settle(&ALICE, Flip::credit(&BOB, 1).into());
		assert_eq!(Flip::total_balance_of(&ALICE), 100);
		assert_eq!(Flip::total_balance_of(&BOB), 50);
		check_balance_integrity();
	});
}

#[test]
fn account_to_external() {
	new_test_ext().execute_with(|| {
		// Account to external
		Flip::settle(&ALICE, Flip::bridge_out(10).into());
		assert_eq!(Flip::total_balance_of(&ALICE), 90);
		assert_eq!(OffchainFunds::<Test>::get(), 860);
		check_balance_integrity();

		// External to account
		Flip::settle(&ALICE, Flip::bridge_in(10).into());
		assert_eq!(Flip::total_balance_of(&ALICE), 100);
		assert_eq!(OffchainFunds::<Test>::get(), 850);
		check_balance_integrity();
	});
}

#[test]
fn mint_external() {
	new_test_ext().execute_with(|| {
		// Mint to external
		mem::drop(Flip::mint(50).offset(Flip::bridge_out(50)));
		assert_eq!(TotalIssuance::<Test>::get(), 1050);
		check_balance_integrity();
		
		mem::drop(Flip::bridge_out(50).offset(Flip::mint(50)));
		assert_eq!(TotalIssuance::<Test>::get(), 1100);
		check_balance_integrity();
	});
}

#[test]
fn burn_external() {
	new_test_ext().execute_with(|| {
		// Burn from external
		mem::drop(Flip::burn(50).offset(Flip::bridge_in(50)));
		assert_eq!(TotalIssuance::<Test>::get(), 950);
		check_balance_integrity();
		
		mem::drop(Flip::bridge_in(50).offset(Flip::burn(50)));
		assert_eq!(TotalIssuance::<Test>::get(), 900);
		check_balance_integrity();
	});
}

#[test]
fn burn_from_account() {
	new_test_ext().execute_with(|| {
		// Burn from account
		Flip::settle(&ALICE, Flip::burn(10).into());
		assert_eq!(Flip::total_balance_of(&ALICE), 90);
		assert_eq!(TotalIssuance::<Test>::get(), 990);
		check_balance_integrity();

	});
}

#[test]
fn mint_to_account() {
	new_test_ext().execute_with(|| {
		// Mint to account
		Flip::settle(&ALICE, Flip::mint(10).into());
		assert_eq!(Flip::total_balance_of(&ALICE), 110);
		assert_eq!(TotalIssuance::<Test>::get(), 1_010);
		check_balance_integrity();
	});
}

#[test]
fn burn_reverts() {
	new_test_ext().execute_with(|| {
		mem::drop(Flip::burn(10));
		assert_eq!(TotalIssuance::<Test>::get(), 1000);
		check_balance_integrity();
	});
}

#[test]
fn mint_reverts() {
	new_test_ext().execute_with(|| {
		mem::drop(Flip::mint(10));
		assert_eq!(TotalIssuance::<Test>::get(), 1000);
		check_balance_integrity();
	});
}

#[test]
fn credit_reverts() {
	new_test_ext().execute_with(|| {
		mem::drop(Flip::credit(&CHARLIE, 1));
		assert_eq!(Flip::total_balance_of(&CHARLIE), 0);
		check_balance_integrity();
	});
}

#[test]
fn debit_reverts() {
	new_test_ext().execute_with(|| {
		mem::drop(Flip::debit(&ALICE, 1));
		assert_eq!(Flip::total_balance_of(&ALICE), 100);
		check_balance_integrity();

		mem::drop(Flip::debit(&ALICE, 1000));
		assert_eq!(Flip::total_balance_of(&ALICE), 100);
		check_balance_integrity();
	});
}

#[test]
fn bridge_in_reverts() {
	new_test_ext().execute_with(|| {
		mem::drop(Flip::bridge_in(100));
		assert_eq!(OffchainFunds::<Test>::get(), 850);
		check_balance_integrity();
	});
}

#[test]
fn bridge_out_reverts() {
	new_test_ext().execute_with(|| {
		mem::drop(Flip::bridge_out(100));
		assert_eq!(OffchainFunds::<Test>::get(), 850);
		check_balance_integrity();
	});
}

#[test]
fn stake_transfers() {
	new_test_ext().execute_with(|| {
		assert_eq!(<Flip as StakeTransfer>::stakeable_balance(&ALICE), 100);
		<Flip as StakeTransfer>::credit_stake(&ALICE, 100);
		assert_eq!(<Flip as StakeTransfer>::stakeable_balance(&ALICE), 200);
		check_balance_integrity();

		// Bond all of it
		Flip::set_validator_bond(&ALICE, 200);
		assert_eq!(<Flip as StakeTransfer>::stakeable_balance(&ALICE), 200);
		assert_eq!(<Flip as StakeTransfer>::claimable_balance(&ALICE), 0);

		// Now try to claim
		assert_noop!(
			<Flip as StakeTransfer>::try_claim(&ALICE, 1),
			Error::<Test>::InsufficientLiquidity
		);

		// Reduce the bond
		Flip::set_validator_bond(&ALICE, 100);
		assert_eq!(<Flip as StakeTransfer>::claimable_balance(&ALICE), 100);
		assert_ok!(<Flip as StakeTransfer>::try_claim(&ALICE, 1));

		check_balance_integrity();
	});
}


#[test]
fn simple_burn() {
	new_test_ext().execute_with(|| {
		// Burn some of Alice's funds
		<Flip as Emissions>::burn_from(&ALICE, 50);
		assert_eq!(Flip::total_balance_of(&ALICE), 50);
		assert_eq!(<Flip as Emissions>::total_issuance(), 950);
		check_balance_integrity();
	});
}

#[test]
fn simple_mint() {
	new_test_ext().execute_with(|| {
		// Mint to Charlie's account.
		<Flip as Emissions>::mint_to(&CHARLIE, 50);
		assert_eq!(Flip::total_balance_of(&CHARLIE), 50);
		assert_eq!(<Flip as Emissions>::total_issuance(), 1050);
		check_balance_integrity();
	});
}

#[test]
fn cant_burn_too_much() {
	new_test_ext().execute_with(|| {
		// Burn some of Alice's funds
		assert_ok!(<Flip as Emissions>::try_burn_from(&ALICE, 50));

		// Try to burn too much - shouldn't succeed
		assert_noop!(
			<Flip as Emissions>::try_burn_from(&ALICE, 51),
			Error::<Test>::InsufficientFunds
		);
		assert_eq!(Flip::total_balance_of(&ALICE), 50);

		// The slashable balance doesn't include the existential deposit.
		assert_eq!(Flip::slashable_funds(&ALICE), 50 - <Test as Config>::ExistentialDeposit::get());

		// Force through a burn of all remaining burnable tokens, including the existential deposit.
		<Flip as Emissions>::burn_from(&ALICE, 51);

		assert_eq!(Flip::total_balance_of(&ALICE), 0);
		assert_eq!(<Flip as Emissions>::total_issuance(), 900);
		check_balance_integrity();
	});
}

#[test]
fn test_vaporise() {
	new_test_ext().execute_with(|| {
		<Flip as Emissions>::vaporise(10);
		assert_eq!(<Flip as Emissions>::total_issuance(), 990);
		check_balance_integrity();

		// Can't vaporise on-chain funds - maximum possible will be taken from offchain.
		<Flip as Emissions>::vaporise(10_000);
		assert_eq!(<Flip as Emissions>::total_issuance(), Flip::onchain_funds());

		check_balance_integrity();
	});
}
