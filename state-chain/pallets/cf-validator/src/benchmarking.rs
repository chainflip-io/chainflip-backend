//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]
use super::*;

use frame_system::RawOrigin;
use frame_benchmarking::{benchmarks, account, whitelisted_caller, impl_benchmark_test_suite};
use sp_std::{vec, vec::Vec, boxed::Box};

#[allow(unused)]
use crate::Pallet as Validator;

benchmarks! {
	set_blocks_for_epoch {
		let s in 2 .. 100;
	}: _(RawOrigin::Root, s.into())
	verify {
		assert_eq!(Pallet::<T>::epoch_number_of_blocks(), s.into())
	}

	set_validator_target_size {
		let s in 2 .. 100;
	}: _(RawOrigin::Root, s.into())
	verify {
		assert_eq!(Pallet::<T>::max_validators(), s)
	}

	force_auction {
		let a in 1 .. 100;
	}: _(RawOrigin::Root)
	verify {
		assert_eq!(Pallet::<T>::force(), true)
	}

	confirm_auction {
		let a in 1 .. 100;
		let e  = EpochIndex(1);
		AuctionToConfirm::<T>::set(Some(e));
	}: _(RawOrigin::Signed(whitelisted_caller()), e)
}

impl_benchmark_test_suite!(
	Pallet,
	crate::mock::new_test_ext(),
	crate::mock::Test,
);



// confirm_auction
