//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_traits::Issuance;
use frame_benchmarking::{benchmarks, impl_benchmark_test_suite, whitelisted_caller};
use frame_system::RawOrigin;
use pallet_cf_flip::FlipIssuance;

#[allow(unused)]
use crate::Pallet as Rewards;

benchmarks! {
	redeem_rewards {
		let caller: T::AccountId = whitelisted_caller();
		// Define use balances
		let rewards_entitlement: T::Balance = T::Balance::from(10000u32);
		let apportioned_rewards: T::Balance = T::Balance::from(2u32);
		let reserved_balance: T::Balance = T::Balance::from(200000u32);
		// Mint to reserve
		let mint = FlipIssuance::<T>::mint(reserved_balance);
		let deposit = Flip::deposit_reserves(VALIDATOR_REWARDS, reserved_balance);
		let _ = mint.offset(deposit);
		// Setup
		Beneficiaries::<T>::insert(VALIDATOR_REWARDS, 4u32);
		RewardsEntitlement::<T>::insert(VALIDATOR_REWARDS, rewards_entitlement);
		ApportionedRewards::<T>::insert(VALIDATOR_REWARDS, &caller, apportioned_rewards);
	}: _(RawOrigin::Signed(caller.clone().into()))
	verify {
		let actual_rewards = ApportionedRewards::<T>::get(&VALIDATOR_REWARDS, caller).expect("ApportionedRewards are none!");
		assert_eq!(T::Balance::from(2500u32), actual_rewards);
	}
}

impl_benchmark_test_suite!(
	Pallet,
	crate::mock::new_test_ext(Default::default(), Default::default()),
	crate::mock::Test,
);
