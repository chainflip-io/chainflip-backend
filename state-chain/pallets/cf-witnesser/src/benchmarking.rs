//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_system::RawOrigin;
use sp_std::{boxed::Box, vec};

benchmarks! {
	witness {
		let caller: T::AccountId = whitelisted_caller();
		let validator_id: T::ValidatorId = caller.clone().into();
		let call: <T as Config>::Call = frame_system::Call::remark{ remark: vec![] }.into();
		let epoch = T::EpochInfo::epoch_index();

		T::EpochInfo::add_authority_info_for_epoch(epoch, vec![validator_id.clone()]);

		// TODO: currently we don't measure the actual execution path
		// we need to set the threshold to 1 to do this.
		// Unfortunately, this is blocked by the fact that we can't pass
		// a witness call here - for now.
	} : _(RawOrigin::Signed(caller.clone()), Box::new(call.clone()))
	verify {
		let call_hash = CallHash(Hashable::blake2_256(&call));
		assert!(Votes::<T>::contains_key(&epoch, &call_hash));
	}

	remove_one_storage_items {
		let call: <T as Config>::Call = frame_system::Call::remark{ remark: vec![] }.into();
		let call_hash = CallHash(Hashable::blake2_256(&call));
		Votes::<T>::insert(0, call_hash, vec![0]);
	} : { let _ = Votes::<T>::clear_prefix(0, u32::MAX, None); }

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
