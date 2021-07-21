use std::mem;

use crate::{
	mock::*, Account as FlipAccount, Config, Error, FlipIssuance, OffchainFunds, TotalIssuance,
};
use cf_traits::{Issuance, StakeTransfer};
use frame_support::traits::{HandleLifetime, Imbalance};
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
fn test_try_debit() {
	new_test_ext().execute_with(|| {
		// Alice's balance is 100, shouldn't be able to debit 101.
		assert!(Flip::try_debit(&ALICE, 101).is_none());
		assert_eq!(Flip::total_balance_of(&ALICE), 100);

		// Charlie's balance is zero, trying to debit or checking the balance should not created the account.
		assert!(Flip::try_debit(&CHARLIE, 1).is_none());
		assert_eq!(Flip::total_balance_of(&CHARLIE), 0);
		assert!(!FlipAccount::<Test>::contains_key(&CHARLIE));

		// Using standard `debit` *does* create an account as a side-effect.
		{
			let zero_surplus = Flip::debit(&CHARLIE, 1);
			assert_eq!(zero_surplus.peek(), 0);
		}
		assert!(FlipAccount::<Test>::contains_key(&CHARLIE));
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

#[cfg(test)]
mod test_issuance {
	use super::*;
	use crate::ReserveId;

	fn burn_from_account(account_id: &AccountId, amount: FlipBalance) {
		Flip::settle_imbalance(account_id, FlipIssuance::<Test>::burn(amount))
	}

	fn mint_to_account(account_id: &AccountId, amount: FlipBalance) {
		Flip::settle_imbalance(account_id, FlipIssuance::<Test>::mint(amount))
	}

	fn burn_from_reserve(reserve_id: ReserveId, amount: FlipBalance) {
		let burn = FlipIssuance::<Test>::burn(amount);
		let withdrawal = Flip::withdraw_reserves(reserve_id, amount);
		let _ = burn.offset(withdrawal);
	}

	fn mint_to_reserve(reserve_id: ReserveId, amount: FlipBalance) {
		let mint = FlipIssuance::<Test>::mint(amount);
		let deposit = Flip::deposit_reserves(reserve_id, amount);
		let _ = mint.offset(deposit);
	}

	#[test]
	fn simple_burn() {
		new_test_ext().execute_with(|| {
			// Burn some of Alice's funds
			burn_from_account(&ALICE, 50);
			assert_eq!(Flip::total_balance_of(&ALICE), 50);
			assert_eq!(FlipIssuance::<Test>::total_issuance(), 950);
			check_balance_integrity();
		});
	}

	#[test]
	fn simple_mint() {
		new_test_ext().execute_with(|| {
			// Mint to Charlie's account.
			mint_to_account(&CHARLIE, 50);
			assert_eq!(Flip::total_balance_of(&CHARLIE), 50);
			assert_eq!(FlipIssuance::<Test>::total_issuance(), 1050);
			check_balance_integrity();
		});
	}

	#[test]
	fn test_reserves() {
		new_test_ext().execute_with(|| {
			const TEST_RESERVE: ReserveId = *b"TEST";
			const INIT_RESERVE_BALANCE: u128 = 0;
			const INIT_TOTAL_ISSUANCE: u128 = 1_000;
			const DEPOSIT: u128 = 50;
			const WITHDRAWAL: u128 = 20;

			// Mint to a reserve.
			mint_to_reserve(TEST_RESERVE, DEPOSIT);
			assert_eq!(Flip::reserved_balance(TEST_RESERVE), INIT_RESERVE_BALANCE + DEPOSIT);
			assert_eq!(FlipIssuance::<Test>::total_issuance(), INIT_TOTAL_ISSUANCE + DEPOSIT);
			check_balance_integrity();

			// Burn some.
			burn_from_reserve(TEST_RESERVE, WITHDRAWAL);
			assert_eq!(Flip::reserved_balance(TEST_RESERVE), INIT_RESERVE_BALANCE + DEPOSIT - WITHDRAWAL);
			assert_eq!(FlipIssuance::<Test>::total_issuance(), INIT_TOTAL_ISSUANCE + DEPOSIT - WITHDRAWAL);

			// Obliterate the rest.
			burn_from_reserve(TEST_RESERVE, 1_000_000);
			assert_eq!(Flip::reserved_balance(TEST_RESERVE), INIT_RESERVE_BALANCE);
			assert_eq!(FlipIssuance::<Test>::total_issuance(), INIT_TOTAL_ISSUANCE);
		});
	}

	#[test]
	fn cant_burn_too_much() {
		new_test_ext().execute_with(|| {
			// Burn some of Alice's funds
			burn_from_account(&ALICE, 50);

			assert_eq!(Flip::total_balance_of(&ALICE), 50);

			// The slashable balance doesn't include the existential deposit.
			assert_eq!(
				Flip::slashable_funds(&ALICE),
				50 - <Test as Config>::ExistentialDeposit::get()
			);

			// Force through a burn of all remaining burnable tokens, including the existential deposit.
			burn_from_account(&ALICE, 1_000_000);

			assert_eq!(Flip::total_balance_of(&ALICE), 0);
			assert_eq!(FlipIssuance::<Test>::total_issuance(), 900);
			check_balance_integrity();
		});
	}

	#[test]
	fn account_deletion_burns_balance() {
		new_test_ext().execute_with(|| {
			frame_system::Provider::<Test>::killed(&BOB).unwrap();
			assert_eq!(FlipIssuance::<Test>::total_issuance(), 950);
			assert_eq!(Flip::total_balance_of(&BOB), 0);
			check_balance_integrity();
		});
	}
}

#[cfg(test)]
mod test_tx_payments {
	use crate::FlipTransactionPayment;
	use frame_support::dispatch::GetDispatchInfo;
	use pallet_transaction_payment::OnChargeTransaction;

	use super::*;

	const CALL: &Call = &Call::System(frame_system::Call::remark(vec![])); // call doesn't matter

	#[test]
	fn test_zero_fee() {
		new_test_ext().execute_with(|| {
			assert!(FlipTransactionPayment::<Test>::withdraw_fee(
				&ALICE,
				CALL,
				&CALL.get_dispatch_info(),
				0,
				0,
			)
			.expect("Alice can afford the fee.")
			.is_none());
		});
	}

	#[test]
	fn test_fee_payment() {
		new_test_ext().execute_with(|| {
			const FEE: FlipBalance = 1;
			const TIP: FlipBalance = 2; // tips should be ignored

			let escrow = FlipTransactionPayment::<Test>::withdraw_fee(
				&ALICE,
				CALL,
				&CALL.get_dispatch_info(),
				FEE,
				TIP,
			)
			.expect("Alice can afford the fee.");

			// Fee is in escrow.
			assert_eq!(escrow.as_ref().map(|fee| fee.peek()), Some(FEE));
			// Issuance unchanged.
			assert_eq!(FlipIssuance::<Test>::total_issuance(), 1000);

			FlipTransactionPayment::<Test>::correct_and_deposit_fee(
				&ALICE,
				&CALL.get_dispatch_info(),
				&().into(),
				FEE,
				TIP,
				escrow,
			)
			.expect("Fee correction never fails.");

			check_balance_integrity();
			// Alice paid the fee.
			assert_eq!(Flip::total_balance_of(&ALICE), 99);
			// Fee was burned.
			assert_eq!(FlipIssuance::<Test>::total_issuance(), 999);
		});
	}

	#[test]
	fn test_fee_unaffordable() {
		new_test_ext().execute_with(|| {
			const FEE: FlipBalance = 101; // what a rip-off
			const TIP: FlipBalance = 2; // tips should be ignored

			FlipTransactionPayment::<Test>::withdraw_fee(
				&ALICE,
				CALL,
				&CALL.get_dispatch_info(),
				FEE,
				TIP,
			)
			.expect_err("Alice can't afford the fee.");

			check_balance_integrity();
			// Alice paid no fee.
			assert_eq!(Flip::total_balance_of(&ALICE), 100);
			// Nothing was burned.
			assert_eq!(FlipIssuance::<Test>::total_issuance(), 1000);
		});
	}

	#[test]
	fn test_partial_refund() {
		new_test_ext().execute_with(|| {
			const PRE_FEE: FlipBalance = 10;
			const POST_FEE: FlipBalance = 7;
			const TIP: FlipBalance = 2; // tips should be ignored

			let escrow = FlipTransactionPayment::<Test>::withdraw_fee(
				&ALICE,
				CALL,
				&CALL.get_dispatch_info(),
				PRE_FEE,
				TIP,
			)
			.expect("Alice can afford the fee.");

			// Fee is in escrow.
			assert_eq!(escrow.as_ref().map(|fee| fee.peek()), Some(PRE_FEE));
			// Issuance unchanged.
			assert_eq!(FlipIssuance::<Test>::total_issuance(), 1000);

			FlipTransactionPayment::<Test>::correct_and_deposit_fee(
				&ALICE,
				&CALL.get_dispatch_info(),
				&().into(),
				POST_FEE,
				TIP,
				escrow,
			)
			.expect("Fee correction never fails.");

			check_balance_integrity();
			// Alice paid the adjusted fee.
			assert_eq!(Flip::total_balance_of(&ALICE), 100 - POST_FEE);
			// The fee was bured.
			assert_eq!(FlipIssuance::<Test>::total_issuance(), 1000 - POST_FEE);
		});
	}
}
