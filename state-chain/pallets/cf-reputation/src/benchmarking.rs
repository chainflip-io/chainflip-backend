//! Benchmarking setup for reputation
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use crate::mock::HEARTBEAT_BLOCK_INTERVAL;
#[allow(unused)]
use crate::Pallet as pallet_cf_reputation;
use frame_benchmarking::account;
use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use frame_system::RawOrigin;
use sp_std::{boxed::Box, vec, vec::Vec};
type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

const VALIDATOR_COUNT: u32 = 150;

benchmarks! {
	heartbeat {
		let b in 0 .. VALIDATOR_COUNT;

		let accounts: Vec<AccountIdOf<T>> = (0..=VALIDATOR_COUNT)
			.map(|i| account("doogle", i, 0))
			.collect();

		pallet_cf_reputation::<T>::on_new_epoch(
			&accounts.iter().map(|a| a.clone().into()).collect(),
			0u32.into(),
		);
	}: _(RawOrigin::Signed(accounts[b as usize].clone()))
	verify {
		let validator_id: T::ValidatorId = accounts[b as usize].clone().into();
		let expected_credits: T::BlockNumber = (HEARTBEAT_BLOCK_INTERVAL as u32).into();
		let reputation_for_validator = pallet_cf_reputation::<T>::reputation(validator_id);
		assert_eq!(reputation_for_validator.online_credits, expected_credits);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::Test;
	use frame_support::assert_ok;
	use sp_io::TestExternalities;

	pub fn new_test_ext() -> TestExternalities {
		let t = frame_system::GenesisConfig::default()
			.build_storage::<Test>()
			.unwrap();
		TestExternalities::new(t)
	}

	#[test]
	fn bench_heartbeat() {
		new_test_ext().execute_with(|| {
			assert_ok!(test_benchmark_heartbeat::<Test>());
		});
	}
}
impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
