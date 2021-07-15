use crate::{mock::*, Error, OnDemandRewardsDistribution, VALIDATOR_REWARDS, ApportionedRewards};
use cf_traits::{Issuance, RewardsDistribution};
use frame_support::{assert_noop, assert_ok};
use pallet_cf_flip::FlipIssuance;

macro_rules! assert_balances {
	( $( $acct:ident => $amt:literal ),+ ) => {
		$(
			assert_eq!(Flip::total_balance_of(&$acct), $amt);
		)+
	};
}

macro_rules! assert_rewards {
	( $( $acct:ident => $received:literal / $due:literal ),+ ) => {
		$(
			assert_eq!(ApportionedRewards::<Test>::get(VALIDATOR_REWARDS, &$acct), $received);
			assert_eq!(FlipRewards::rewards_due(&$acct), $due);
		)+
	};
}

fn simulate_emissions_distribution(amount: u128) {
	OnDemandRewardsDistribution::<Test>::distribute(FlipIssuance::mint(amount));
}

fn assert_reserved_balance(expected_amount: u128) {
	assert_eq!(Flip::reserved_balance(VALIDATOR_REWARDS), expected_amount);
}

#[test]
fn test_basics() {
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
