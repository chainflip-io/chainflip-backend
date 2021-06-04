//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_system::RawOrigin;
use frame_benchmarking::{benchmarks, whitelisted_caller, impl_benchmark_test_suite};
use sp_std::{vec, vec::Vec, boxed::Box};

#[allow(unused)]
use crate::Pallet as Validator;

benchmarks! {
	set_blocks_for_epoch {
		let b = 2_u32;
	}: _(RawOrigin::Root, b.into())
	verify {
		assert_eq!(Pallet::<T>::epoch_number_of_blocks(), 2_u32.into())
	}

	set_validator_target_size {
	}: _(RawOrigin::Root, 10_u32.into())
	verify {
		assert_eq!(Pallet::<T>::max_validators(), 10_u32)
	}

	force_auction {
	}: _(RawOrigin::Root)
	verify {
		assert_eq!(Pallet::<T>::force(), true)
	}

	confirm_auction {
		AuctionToConfirm::<T>::set(Some(EpochIndex(1)));
	}: _(RawOrigin::Signed(whitelisted_caller()), EpochIndex(1))
}

impl_benchmark_test_suite!(
	Pallet,
	crate::mock::new_test_ext(),
	crate::mock::Test,
);