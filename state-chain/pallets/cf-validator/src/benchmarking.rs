//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{benchmarks, impl_benchmark_test_suite, whitelisted_caller};
use frame_system::RawOrigin;

#[allow(unused)]
use crate::Pallet as Validator;

benchmarks! {
	set_blocks_for_epoch {
		let b = 2_u32;
	}: _(RawOrigin::Root, b.into())
	verify {
		assert_eq!(Pallet::<T>::epoch_number_of_blocks(), 2_u32.into())
	}
	force_rotation {
	}: _(RawOrigin::Root)
	verify {
		assert_eq!(Pallet::<T>::force(), true)
	}
	cfe_version {
		let caller: T::AccountId = whitelisted_caller();
		let version = SemVer {
			major: 1,
			minor: 2,
			patch: 3
		};
	}: _(RawOrigin::Signed(caller.clone()), version.clone())
	verify {
		let validator_id: T::ValidatorId = caller.into();
		assert_eq!(Pallet::<T>::validator_cfe_version(validator_id), version)
	}
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
