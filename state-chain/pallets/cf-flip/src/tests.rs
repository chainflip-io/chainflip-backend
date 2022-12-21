use std::mem;

use crate::{
	mock::*, Account, Bonder, Error, FlipAccount, FlipIssuance, FlipSlasher, OffchainFunds,
	Reserve, SlashingRate, TotalIssuance,
};
use cf_primitives::FlipBalance;
use cf_traits::{Bonding, Issuance, Slashing, StakeTransfer};
use frame_support::{
	assert_noop,
	traits::{HandleLifetime, Hooks, Imbalance},
};
use quickcheck::{Arbitrary, Gen, TestResult};
use quickcheck_macros::quickcheck;
use sp_runtime::Permill;

impl FlipOperation {
	pub fn execute(&self) -> bool {
		match self {
			// Mint to external
			FlipOperation::MintExternal(amount_1, amount_2) => {
				let previous_issuance = TotalIssuance::<Test>::get();
				let previous_offchain_funds = OffchainFunds::<Test>::get();
				mem::drop(Flip::mint(*amount_1).offset(Flip::bridge_out(*amount_1)));
				let intermediate_issuance = TotalIssuance::<Test>::get();
				let intermediate_offchain_funds = OffchainFunds::<Test>::get();
				if intermediate_issuance !=
					(previous_issuance.checked_add(*amount_1).unwrap_or(previous_issuance)) ||
					intermediate_offchain_funds !=
						previous_offchain_funds + (intermediate_issuance - previous_issuance)
				{
					return false
				}
				mem::drop(Flip::bridge_out(*amount_2).offset(Flip::mint(*amount_2)));
				let final_offchain_funds = OffchainFunds::<Test>::get();
				let final_issuance = TotalIssuance::<Test>::get();
				if final_issuance !=
					(intermediate_issuance
						.checked_add(*amount_2)
						.unwrap_or(intermediate_issuance)) ||
					final_offchain_funds !=
						intermediate_offchain_funds + (final_issuance - intermediate_issuance)
				{
					return false
				}
			},
			// Burn from external
			FlipOperation::BurnExternal(amount_1, amount_2) => {
				let previous_offchain_funds = OffchainFunds::<Test>::get();
				let previous_issuance = TotalIssuance::<Test>::get();
				mem::drop(Flip::burn(*amount_1).offset(Flip::bridge_in(*amount_1)));
				let intermediate_offchain_funds = OffchainFunds::<Test>::get();
				let intermediate_issuance = TotalIssuance::<Test>::get();
				if intermediate_issuance !=
					(previous_issuance - (previous_offchain_funds - intermediate_offchain_funds)) ||
					intermediate_offchain_funds !=
						previous_offchain_funds.saturating_sub(*amount_1)
				{
					return false
				}
				mem::drop(Flip::bridge_in(*amount_2).offset(Flip::burn(*amount_2)));
				let final_offchain_funds = OffchainFunds::<Test>::get();
				let final_issuance = TotalIssuance::<Test>::get();
				if final_issuance !=
					(intermediate_issuance -
						(intermediate_offchain_funds - final_offchain_funds)) ||
					final_offchain_funds != intermediate_offchain_funds.saturating_sub(*amount_2)
				{
					return false
				}
			},
			FlipOperation::BurnReverts(amount) => {
				let previous_issuance = TotalIssuance::<Test>::get();
				mem::drop(Flip::burn(*amount));
				if TotalIssuance::<Test>::get() != previous_issuance {
					return false
				}
			},
			FlipOperation::MintReverts(amount) => {
				let previous_issuance = TotalIssuance::<Test>::get();
				mem::drop(Flip::mint(*amount));
				if TotalIssuance::<Test>::get() != previous_issuance {
					return false
				}
			},
			FlipOperation::CreditReverts(amount) => {
				let previous_balance = Flip::total_balance_of(&CHARLIE);
				mem::drop(Flip::credit(&CHARLIE, *amount));
				if Flip::total_balance_of(&CHARLIE) != previous_balance {
					return false
				}
			},
			FlipOperation::DebitReverts(amount) => {
				let previous_balance = Flip::total_balance_of(&ALICE);
				mem::drop(Flip::debit(&ALICE, *amount));
				if Flip::total_balance_of(&ALICE) != previous_balance {
					return false
				}
			},
			FlipOperation::BridgeInReverts(amount) => {
				let previous_offchain_funds = OffchainFunds::<Test>::get();
				mem::drop(Flip::bridge_in(*amount));
				if OffchainFunds::<Test>::get() != previous_offchain_funds {
					return false
				}
			},
			FlipOperation::BridgeOutReverts(amount) => {
				let previous_offchain_funds = OffchainFunds::<Test>::get();
				mem::drop(Flip::bridge_out(*amount));
				if OffchainFunds::<Test>::get() != previous_offchain_funds {
					return false
				}
			},
			// Mint To Reserve
			FlipOperation::MintToReserve(amount) => {
				use crate::ReserveId;
				const TEST_RESERVE: ReserveId = *b"TEST";
				let previous_issuance = TotalIssuance::<Test>::get();
				let previous_reserve = Reserve::<Test>::try_get(TEST_RESERVE).unwrap_or(0);

				let mint = FlipIssuance::<Test>::mint(*amount);
				let deposit = Flip::deposit_reserves(TEST_RESERVE, *amount);
				mem::drop(mint.offset(deposit));

				let new_issuance = TotalIssuance::<Test>::get();
				let new_reserve = Reserve::<Test>::try_get(TEST_RESERVE).unwrap_or(0);
				if new_issuance !=
					previous_issuance.checked_add(*amount).unwrap_or(previous_issuance)
				{
					return false
				}
				if new_reserve != previous_reserve + (new_issuance - previous_issuance) {
					return false
				}
			},
			// Burn From Reserve
			FlipOperation::BurnFromReserve(amount) => {
				use crate::ReserveId;
				const TEST_RESERVE: ReserveId = *b"TEST";
				let previous_issuance = TotalIssuance::<Test>::get();
				let previous_reserve = Reserve::<Test>::try_get(TEST_RESERVE).unwrap_or(0);

				let burn = FlipIssuance::<Test>::burn(*amount);
				let withdrawal = Flip::withdraw_reserves(TEST_RESERVE, *amount);
				let _result = burn.offset(withdrawal);

				if Flip::reserved_balance(TEST_RESERVE) != previous_reserve.saturating_sub(*amount)
				{
					return false
				}
				if FlipIssuance::<Test>::total_issuance() !=
					previous_issuance.saturating_sub(*amount)
				{
					return false
				}
			},
			// Burn From Account
			FlipOperation::BurnFromAccount(account_id, amount) => {
				let previous_balance = Flip::total_balance_of(account_id);
				let previous_issuance = TotalIssuance::<Test>::get();
				Flip::settle(account_id, Flip::burn(*amount).into());
				let new_balance = Flip::total_balance_of(account_id);
				if new_balance != previous_balance.saturating_sub(*amount) {
					return false
				}
				if TotalIssuance::<Test>::get() !=
					previous_issuance - (previous_balance - new_balance)
				{
					return false
				}
			},
			// Mint To Account
			FlipOperation::MintToAccount(account_id, amount) => {
				let previous_balance = Flip::total_balance_of(account_id);
				let previous_issuance = TotalIssuance::<Test>::get();
				Flip::settle(account_id, Flip::mint(*amount).into());
				if TotalIssuance::<Test>::get() !=
					previous_issuance.checked_add(*amount).unwrap_or(previous_issuance)
				{
					return false
				}
				if Flip::total_balance_of(account_id) !=
					previous_balance + (TotalIssuance::<Test>::get() - previous_issuance)
				{
					return false
				}
			},
			// Transfer out of Account to offchain funds
			FlipOperation::ExternalTransferOut(account_id, amount) => {
				let previous_balance = Flip::total_balance_of(account_id);
				let previous_offchain_funds = OffchainFunds::<Test>::get();
				Flip::settle(account_id, Flip::bridge_out(*amount).into());
				let new_balance = Flip::total_balance_of(account_id);
				let new_offchain_funds = OffchainFunds::<Test>::get();
				match previous_offchain_funds.checked_add(*amount) {
					Some(_sum) => {
						if new_balance != previous_balance.saturating_sub(*amount) ||
							new_offchain_funds !=
								previous_offchain_funds + (previous_balance - new_balance)
						{
							return false
						}
					},
					None => {
						if new_balance != previous_balance ||
							new_offchain_funds != previous_offchain_funds
						{
							return false
						}
					},
				}
			},
			// Transfer into Account from Offchain funds
			FlipOperation::ExternalTransferIn(account_id, amount) => {
				let previous_balance = Flip::total_balance_of(account_id);
				let previous_offchain_funds = OffchainFunds::<Test>::get();
				Flip::settle(account_id, Flip::bridge_in(*amount).into());
				let new_balance = Flip::total_balance_of(account_id);
				let new_offchain_funds = OffchainFunds::<Test>::get();
				if new_balance != previous_balance + (previous_offchain_funds - new_offchain_funds)
				{
					return false
				}
				if new_offchain_funds != previous_offchain_funds.saturating_sub(*amount) {
					return false
				}
			},
			// Update stake, Bond and claim
			FlipOperation::UpdateStakeAndBond(account_id, stake, bond) => {
				// Update Stake
				let previous_stake = <Flip as StakeTransfer>::staked_balance(account_id);
				let previous_offchain_funds = OffchainFunds::<Test>::get();
				<Flip as StakeTransfer>::credit_stake(account_id, *stake);
				let new_stake = <Flip as StakeTransfer>::staked_balance(account_id);
				let new_offchain_funds = OffchainFunds::<Test>::get();
				if new_offchain_funds != previous_offchain_funds.saturating_sub(*stake) ||
					new_stake !=
						(previous_stake + (previous_offchain_funds - new_offchain_funds)) ||
					!MockStakeHandler::has_stake_updated(account_id)
				{
					return false
				}

				// Bond all of it
				Bonder::<Test>::update_bond(account_id, new_stake);
				if new_stake != (previous_stake + (previous_offchain_funds - new_offchain_funds)) ||
					<Flip as StakeTransfer>::claimable_balance(account_id) != 0
				{
					return false
				}

				// Now try to claim
				assert_noop!(
					<Flip as StakeTransfer>::try_initiate_claim(account_id, 1),
					Error::<Test>::InsufficientLiquidity
				);

				// Reduce the bond
				Bonder::<Test>::update_bond(account_id, *bond);
				let expected_claimable_balance = new_stake.saturating_sub(*bond);
				if <Flip as StakeTransfer>::claimable_balance(account_id) !=
					expected_claimable_balance
				{
					return false
				}
				assert!(
					<Flip as StakeTransfer>::try_initiate_claim(
						account_id,
						expected_claimable_balance
					)
					.is_ok(),
					"expexted: {}, claimable: {}",
					expected_claimable_balance,
					<Flip as StakeTransfer>::claimable_balance(account_id)
				);
				<Flip as StakeTransfer>::finalize_claim(account_id)
					.expect("Pending Claim should exist");
				if !MockStakeHandler::has_stake_updated(account_id) {
					return false
				}
			},
			FlipOperation::SlashAccount(account_id, slashing_rate, bond, mint, blocks) => {
				// Mint some Flip for testing - 100 is not enough and unrealistic for this usecase
				Flip::settle(account_id, Flip::mint(*mint).into());
				let initial_balance: u128 = Flip::total_balance_of(account_id);
				Bonder::<Test>::update_bond(account_id, *bond);

				SlashingRate::<Test>::set(*slashing_rate);

				let attempted_slash: u128 =
					(*slashing_rate * *bond as u128).saturating_mul((*blocks).into());
				let expected_slash =
					if Account::<Test>::get(account_id).can_be_slashed(attempted_slash) {
						attempted_slash
					} else {
						0
					};

				FlipSlasher::<Test>::slash(account_id, *blocks);
				let balance_after = Flip::total_balance_of(account_id);
				// Check if the diff between the balances is the expected slash
				if initial_balance.saturating_sub(expected_slash) != balance_after {
					return false
				}
				if expected_slash > 0 {
					System::assert_last_event(RuntimeEvent::Flip(
						crate::Event::<Test>::SlashingPerformed {
							who: *account_id,
							amount: expected_slash,
						},
					));
				}
			},
			// Account to account transfer
			FlipOperation::AccountToAccount(account_id_1, account_id_2, amount_1, amount_2) => {
				let previous_balance_account_1 = Flip::total_balance_of(account_id_1);
				let previous_balance_account_2 = Flip::total_balance_of(account_id_2);

				// Transfer amount_1 by creating a debit imbalance
				Flip::settle(account_id_1, Flip::debit(account_id_2, *amount_1).into());
				let intermediate_balance_account_1 = Flip::total_balance_of(account_id_1);
				let intermediate_balance_account_2 = Flip::total_balance_of(account_id_2);
				if account_id_1 != account_id_2 {
					if intermediate_balance_account_1 !=
						previous_balance_account_1 +
							(previous_balance_account_2 - intermediate_balance_account_2) ||
						intermediate_balance_account_2 !=
							previous_balance_account_2.saturating_sub(*amount_1)
					{
						return false
					}
				} else if intermediate_balance_account_1 != previous_balance_account_1 {
					return false
				}

				// Transfer amount_2 by creating a credit imbalance
				// Note: Extra checked_add check in this case due to the fact that credit imbalance
				// might be 0 due to saturating add reverting even though the real transfer amount
				// would have been valid. This edge case might need to be addressed properly in Flip
				// pallet code
				Flip::settle(account_id_1, Flip::credit(account_id_2, *amount_2).into());
				let final_balance_account_1 = Flip::total_balance_of(account_id_1);
				let final_balance_account_2 = Flip::total_balance_of(account_id_2);
				if account_id_1 != account_id_2 &&
					intermediate_balance_account_2.checked_add(*amount_2).is_some()
				{
					if final_balance_account_2 !=
						intermediate_balance_account_2 +
							(intermediate_balance_account_1 - final_balance_account_1) ||
						final_balance_account_1 !=
							intermediate_balance_account_1.saturating_sub(*amount_2)
					{
						return false
					}
				} else if final_balance_account_1 != intermediate_balance_account_1 ||
					final_balance_account_2 != intermediate_balance_account_2
				{
					return false
				}
			},
		}
		true
	}
}

impl Arbitrary for FlipOperation {
	fn arbitrary(g: &mut Gen) -> FlipOperation {
		let operation_choice = u128::arbitrary(g) % 17;
		match operation_choice {
			0 => FlipOperation::MintExternal(u128::arbitrary(g), u128::arbitrary(g)),
			1 => FlipOperation::BurnExternal(u128::arbitrary(g), u128::arbitrary(g)),
			2 => FlipOperation::BurnReverts(u128::arbitrary(g)),
			3 => FlipOperation::MintReverts(u128::arbitrary(g)),
			4 => FlipOperation::CreditReverts(u128::arbitrary(g)),
			5 => FlipOperation::DebitReverts(u128::arbitrary(g)),
			6 => FlipOperation::BridgeInReverts(u128::arbitrary(g)),
			7 => FlipOperation::BridgeOutReverts(u128::arbitrary(g)),
			8 => FlipOperation::MintToReserve(u128::arbitrary(g)),
			9 => FlipOperation::BurnFromReserve(u128::arbitrary(g)),
			10 => FlipOperation::BurnFromAccount(random_account(g), u128::arbitrary(g)),
			11 => FlipOperation::MintToAccount(random_account(g), u128::arbitrary(g)),
			12 => FlipOperation::ExternalTransferOut(random_account(g), u128::arbitrary(g)),
			13 => FlipOperation::ExternalTransferIn(random_account(g), u128::arbitrary(g)),
			14 => FlipOperation::UpdateStakeAndBond(
				random_account(g),
				u128::arbitrary(g),
				u128::arbitrary(g),
			),
			15 => FlipOperation::SlashAccount(
				random_account(g),
				Permill::from_rational(u32::arbitrary(g), u32::MAX),
				Bond::arbitrary(g),
				Mint::arbitrary(g),
				(u16::arbitrary(g) as u32).into(), // random number of blocks up to u16::MAX
			),
			16 => FlipOperation::AccountToAccount(
				random_account(g),
				random_account(g),
				u128::arbitrary(g),
				u128::arbitrary(g),
			),
			_ => unreachable!(),
		}
	}
}

fn random_account(g: &mut Gen) -> u64 {
	match u8::arbitrary(g) % 3 {
		0 => ALICE,
		1 => BOB,
		2 => CHARLIE,
		_ => unreachable!(),
	}
}

#[quickcheck]
fn balance_has_integrity(events: Vec<FlipOperation>) -> TestResult {
	new_test_ext().execute_with(|| -> TestResult {
		if events.iter().any(|event| !event.execute()) || !check_balance_integrity() {
			TestResult::failed()
		} else {
			TestResult::passed()
		}
	})
}

#[test]
fn test_try_debit() {
	new_test_ext().execute_with(|| {
		// Alice's balance is 100, shouldn't be able to debit 101.
		assert!(Flip::try_debit(&ALICE, 101).is_none());
		assert_eq!(Flip::total_balance_of(&ALICE), 100);

		// Charlie's balance is zero, trying to debit or checking the balance should not created the
		// account.
		assert!(Flip::try_debit(&CHARLIE, 1).is_none());
		assert_eq!(Flip::total_balance_of(&CHARLIE), 0);
		assert!(!Account::<Test>::contains_key(&CHARLIE));

		// Using standard `debit` *does* create an account as a side-effect.
		{
			let zero_surplus = Flip::debit(&CHARLIE, 1);
			assert_eq!(zero_surplus.peek(), 0);
		}
		assert!(Account::<Test>::contains_key(&CHARLIE));
	});
}

#[test]
fn test_try_debit_from_liquid_funds() {
	new_test_ext().execute_with(|| {
		// Ensure the initial balance of ALICE
		assert_eq!(Flip::total_balance_of(&ALICE), 100);
		// Bond the account
		Bonder::<Test>::update_bond(&ALICE, 50);
		// Try to debit more than liquid funds available in the account
		assert!(Flip::try_debit_from_liquid_funds(&ALICE, 60).is_none());
		// Try to debit less and burn the fee
		Flip::try_debit_from_liquid_funds(&ALICE, 10)
			.expect("Debit of funds failed!")
			.offset(Flip::burn(10));
		// Expect the account balance to be reduced
		assert_eq!(Flip::total_balance_of(&ALICE), 90);
	});
}

#[cfg(test)]
mod test_issuance {
	use super::*;

	#[test]
	fn account_deletion_burns_balance() {
		new_test_ext().execute_with(|| {
			frame_system::Provider::<Test>::killed(&BOB).unwrap();
			assert_eq!(FlipIssuance::<Test>::total_issuance(), 950);
			assert_eq!(Flip::total_balance_of(&BOB), 0);
			assert!(check_balance_integrity());
		});
	}
}

#[cfg(test)]
mod test_tx_payments {
	use crate::FlipTransactionPayment;
	use frame_support::{dispatch::GetDispatchInfo, pallet_prelude::InvalidTransaction};
	use pallet_transaction_payment::OnChargeTransaction;

	use super::*;

	const CALL: &RuntimeCall = &RuntimeCall::System(frame_system::Call::remark { remark: vec![] }); // call doesn't matter

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

	fn test_invalid_account(fee: FlipBalance) {
		// A really naughty dude, don't trust him.
		const BEELZEBUB: AccountId = 666;
		new_test_ext().execute_with(|| {
			assert_eq!(
				FlipTransactionPayment::<Test>::withdraw_fee(
					&BEELZEBUB,
					CALL,
					&CALL.get_dispatch_info(),
					fee,
					0,
				)
				.expect_err("Account doesn't exist. Expected error, got"),
				InvalidTransaction::Payment.into()
			);
		});
	}

	#[test]
	fn test_invalid_no_fee() {
		test_invalid_account(0)
	}

	#[test]
	fn test_invalid_with_fee() {
		test_invalid_account(1)
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

			assert!(check_balance_integrity());
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

			assert!(check_balance_integrity());
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

			assert!(check_balance_integrity());
			// Alice paid the adjusted fee.
			assert_eq!(Flip::total_balance_of(&ALICE), 100 - POST_FEE);
			// The fee was bured.
			assert_eq!(FlipIssuance::<Test>::total_issuance(), 1000 - POST_FEE);
		});
	}
}

#[test]
fn can_reap_dust_account() {
	new_test_ext().execute_with(|| {
		Account::<Test>::insert(ALICE, FlipAccount { stake: 9, bond: 0 });
		Account::<Test>::insert(BOB, FlipAccount { stake: 10, bond: 0 });
		Account::<Test>::insert(CHARLIE, FlipAccount { stake: 11, bond: 0 });

		// Dust accounts are reaped on_idle
		Flip::on_idle(1, 1_000_000_000_000);

		assert!(!Account::<Test>::contains_key(ALICE));
		assert_eq!(Account::<Test>::get(BOB), FlipAccount { stake: 10, bond: 0 });

		assert_eq!(Account::<Test>::get(CHARLIE), FlipAccount { stake: 11, bond: 0 });
		System::assert_has_event(RuntimeEvent::Flip(crate::Event::AccountReaped {
			who: ALICE,
			dust_burned: 9,
		}));
		System::assert_last_event(RuntimeEvent::System(frame_system::Event::KilledAccount {
			account: ALICE,
		}));
	})
}
