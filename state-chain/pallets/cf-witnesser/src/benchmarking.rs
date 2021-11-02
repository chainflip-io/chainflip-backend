//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{benchmarks, impl_benchmark_test_suite, whitelisted_caller};
use frame_system::RawOrigin;
use sp_std::{boxed::Box, vec, vec::Vec};

#[allow(unused)]
use crate::Pallet as Auction;

benchmarks! {
	witness {
		let caller: T::AccountId = whitelisted_caller();
		let validator_id: T::ValidatorId = caller.clone().into();
		let call: <T as Config>::Call = frame_system::Call::remark(vec![]).into();
		let epoch = T::EpochInfo::epoch_index();
		ValidatorIndex::<T>::insert(&epoch, caller.clone(), 0 as u16);
		NumValidators::<T>::set(1);
		// TODO: currently we don't measure the actual execution path
		// we need to set the threshold to 1 to do this.
		// Unfortunately, this is blocked by the fact that we can't pass
		// a witness call here - for now.
		ConsensusThreshold::<T>::set(2);
	} : _(RawOrigin::Signed(caller.clone()), Box::new(call))
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
