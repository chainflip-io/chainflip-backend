use crate::{mock::*, ApportionedRewards, Error, OnDemandRewardsDistribution, VALIDATOR_REWARDS};
use cf_traits::{Issuance, RewardsDistribution};
use frame_support::{assert_noop, assert_ok};
use frame_system::RawOrigin;
use pallet_cf_flip::FlipIssuance;

/// Helper macro to check multiple balances.
macro_rules! assert_balances {
	( $( $acct:ident => $amt:literal ),+ ) => {
		$(
			assert_eq!(Flip::total_balance_of(&$acct), $amt);
		)+
	};
}

/// Check the expected values for rewards received and rewards still due.
///
/// For example, the following checks that Alice has received none of her 50 rewards, and that BOB has received 50 and
/// no more are due to him:
///
/// ```
/// assert_rewards!(ALICE => 0 / 50, BOB => 50 / 0);
/// ```
macro_rules! assert_rewards {
	( $( $acct:ident => $received:literal / $due:expr ),+ ) => {
		$(
			let received = ApportionedRewards::<Test>::get(VALIDATOR_REWARDS, &$acct).unwrap_or(0);
			assert_eq!(received, $received, "Expected apportionment of {}, got {}.", $received, received);
			let due = FlipRewards::rewards_due(&$acct);
			assert_eq!(FlipRewards::rewards_due(&$acct), $due, "Expected entitlement of {}, got {}.", $due, due);
		)+
	};
}

fn simulate_emissions_distribution(amount: u128) {
	OnDemandRewardsDistribution::<Test>::distribute(FlipIssuance::mint(amount));
}

fn assert_reserved_balance(expected_amount: u128) {
	let amount = Flip::reserved_balance(VALIDATOR_REWARDS);
	assert_eq!(
		amount, expected_amount,
		"Reserves did not match expected amount: Got {}, expected {}",
		amount, expected_amount
	);
}

#[cfg(test)]
mod test_rewards_due {
	use super::*;

	fn test_with(
		emission: u128,
		balances: Vec<(AccountId, u128)>,
		account_id: AccountId,
		expected_balance: u128,
	) {
		new_test_ext(Some(1_000), balances).execute_with(|| {
			simulate_emissions_distribution(emission);
			let due = FlipRewards::rewards_due(&account_id);
			assert_eq!(
				due, expected_balance,
				"Expected account to be due {}, but got {}",
				expected_balance, due
			);
		});
	}

	#[test]
	fn test_nothing_due() {
		test_with(0, vec![(ALICE, 100)], ALICE, 0);
	}

	#[test]
	fn test_everything_due() {
		test_with(10, vec![(ALICE, 100)], ALICE, 10);
	}

	#[test]
	fn test_50_50_split() {
		test_with(10, vec![(ALICE, 100), (BOB, 100)], ALICE, 5);
	}

	#[test]
	fn test_uneven_split() {
		test_with(11, vec![(ALICE, 100), (BOB, 100)], ALICE, 5);
		test_with(11, vec![(ALICE, 100), (BOB, 100)], BOB, 5);
	}

	#[test]
	fn test_non_beneficiary() {
		test_with(10, vec![(ALICE, 100), (BOB, 100)], CHARLIE, 0);
	}
}

#[test]
fn test_basic_distribution() {
	new_test_ext(Some(1_000), vec![(ALICE, 100), (BOB, 100)]).execute_with(|| {
		simulate_emissions_distribution(100);
		assert_eq!(Flip::total_issuance(), 1_100);
		assert_balances![
			ALICE => 100,
			BOB => 100
		];
		assert_rewards![
			ALICE => 0 / 50,
			BOB => 0 / 50
		];
		assert_reserved_balance(100);
		check_balance_integrity();
	});
}

#[test]
fn test_multiple_emissions_then_single_claim() {
	const START_ISSUANCE: u128 = 1_000;
	let mut emissions = 0;

	let mut one_emissions_cycle = |emitted| {
		emissions += emitted;

		simulate_emissions_distribution(emitted);
		assert_eq!(Flip::total_issuance(), START_ISSUANCE + emissions);
		assert_balances![
			ALICE => 100,
			BOB => 100
		];
		assert_rewards![
			ALICE => 0 / (emissions / 2),
			BOB => 0 / (emissions / 2)
		];
		assert_reserved_balance(emissions);
		check_balance_integrity();
	};

	new_test_ext(Some(START_ISSUANCE), vec![(ALICE, 100), (BOB, 100)]).execute_with(|| {
		for _ in 0..5 {
			one_emissions_cycle(10);
		}

		// Alice cashes in her rewards. She can only do this once until more rewards are emitted.
		assert_ok!(FlipRewards::redeem_rewards(RawOrigin::Signed(ALICE).into()));
		assert_noop!(
			FlipRewards::redeem_rewards(RawOrigin::Signed(ALICE).into()),
			Error::<Test>::NoRewardEntitlement
		);

		assert_eq!(Flip::total_issuance(), 1_050);
		assert_balances![
			ALICE => 125,
			BOB => 100
		];
		assert_rewards![
			ALICE => 25 / 0,
			BOB => 0 / 25
		];
		assert_reserved_balance(25);
		check_balance_integrity();
	});
}

fn apportion_all() {
	for (account_id, already_received) in ApportionedRewards::<Test>::iter_prefix(VALIDATOR_REWARDS)
	{
		FlipRewards::apportion_amount(
			&account_id,
			FlipRewards::rewards_due_each() - already_received,
		);
	}
}

#[test]
fn test_full_apportionment_even() {
	const START_ISSUANCE: u128 = 1_000;

	new_test_ext(Some(START_ISSUANCE), vec![(ALICE, 100), (BOB, 100)]).execute_with(|| {
		simulate_emissions_distribution(50);

		// Apportion all the rewards.
		apportion_all();

		assert_eq!(Flip::total_issuance(), 1_050);
		assert_balances![
			ALICE => 125,
			BOB => 125
		];
		assert_rewards![
			ALICE => 25 / 0,
			BOB => 25 / 0
		];
		assert_reserved_balance(0);
		check_balance_integrity();
	});
}

#[test]
fn test_full_apportionment_uneven() {
	const START_ISSUANCE: u128 = 1_000;

	new_test_ext(Some(START_ISSUANCE), vec![(ALICE, 100), (BOB, 100)]).execute_with(|| {
		simulate_emissions_distribution(51);

		// Apportion all the rewards.
		apportion_all();

		assert_eq!(Flip::total_issuance(), 1_051);
		assert_balances![
			ALICE => 125,
			BOB => 125
		];
		assert_rewards![
			ALICE => 25 / 0,
			BOB => 25 / 0
		];
		assert_reserved_balance(1);
		check_balance_integrity();
	});
}

#[test]
fn test_rollover() {
	const START_ISSUANCE: u128 = 1_000;

	new_test_ext(Some(START_ISSUANCE), vec![(ALICE, 100), (BOB, 100)]).execute_with(|| {
		simulate_emissions_distribution(51);
		assert_eq!(Flip::total_issuance(), 1_051);
		assert_balances![
			ALICE => 100,
			BOB => 100,
			CHARLIE => 0
		];
		assert_rewards![
			ALICE => 0 / 25,
			BOB => 0 / 25,
			CHARLIE => 0 / 0
		];
		assert_reserved_balance(51);
		check_balance_integrity();

		// Do a rollover.
		assert_ok!(FlipRewards::rollover(&vec![CHARLIE, ALICE]));

		// Rewards should be fully distributed and entitlements reset to zero
		assert_eq!(Flip::total_issuance(), 1_051);
		assert_balances![
			ALICE => 125,
			BOB => 125,
			CHARLIE => 0
		];
		assert_rewards![
			ALICE => 0 / 0,
			CHARLIE => 0 / 0
		];
		assert_reserved_balance(1);
		check_balance_integrity();

		// Bob is no longer in the set of beneficiaries.
		// The remaining 1 FLIP from the previous cycle should have rolled over to the new one.
		simulate_emissions_distribution(49);
		assert_eq!(Flip::total_issuance(), 1_100);
		assert_balances![
			ALICE => 125,
			BOB => 125,
			CHARLIE => 0
		];
		assert_rewards![
			ALICE => 0 / 25,
			BOB => 0 / 0,
			CHARLIE => 0 / 25
		];
		assert_reserved_balance(50);
		check_balance_integrity();

		// Do another rollover.
		assert_ok!(FlipRewards::rollover(&vec![]));

		assert_eq!(Flip::total_issuance(), 1_100);
		assert_balances![
			ALICE => 150,
			BOB => 125,
			CHARLIE => 25
		];
		assert_rewards![
			ALICE => 0 / 0,
			BOB => 0 / 0,
			CHARLIE => 0 / 0
		];
		assert_reserved_balance(0);
		check_balance_integrity();
	});
}
