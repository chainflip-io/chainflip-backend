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
		let caller = whitelisted_caller();
		let rewards_entitlement: T::Balance = T::Balance::from(1000000000 as u32);
		let apportioned_rewards: T::Balance = T::Balance::from(2 as u32);
		let reserved_balance: T::Balance = T::Balance::from(2 as u32);
		let _ = FlipIssuance::<T>::mint(reserved_balance);
		Beneficiaries::<T>::insert(VALIDATOR_REWARDS, 4 as u32);
		RewardsEntitlement::<T>::insert(VALIDATOR_REWARDS, rewards_entitlement);
		ApportionedRewards::<T>::insert(VALIDATOR_REWARDS, &caller, apportioned_rewards);
	}: _(RawOrigin::Signed(caller))
}

impl_benchmark_test_suite!(
	Pallet,
	crate::mock::new_test_ext(Default::default(), Default::default()),
	crate::mock::Test,
);
