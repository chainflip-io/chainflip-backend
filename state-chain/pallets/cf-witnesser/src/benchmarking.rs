//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_primitives::AccountRole;
use cf_traits::AccountRoleRegistry;
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::traits::Hooks;
use frame_system::RawOrigin;
use sp_std::{boxed::Box, vec};

benchmarks! {
	witness_at_epoch {
		let caller: T::AccountId = whitelisted_caller();
		let validator_id: T::ValidatorId = caller.clone().into();
		T::AccountRoleRegistry::register_account(caller.clone(), AccountRole::Validator);
		let call: <T as Config>::RuntimeCall = frame_system::Call::remark{ remark: vec![] }.into();
		let epoch = T::EpochInfo::epoch_index();

		T::EpochInfo::add_authority_info_for_epoch(epoch, vec![validator_id]);

		// TODO: currently we don't measure the actual execution path
		// we need to set the threshold to 1 to do this.
		// Unfortunately, this is blocked by the fact that we can't pass
		// a witness call here - for now.
	} : _(RawOrigin::Signed(caller.clone()), Box::new(call.clone()), epoch)
	verify {
		let call_hash = CallHash(Hashable::blake2_256(&call));
		assert!(Votes::<T>::contains_key(epoch, call_hash));
	}

	remove_storage_items {
		let n in 1u32 .. 255u32;

		for i in 0..n {
			let call: <T as Config>::RuntimeCall = frame_system::Call::remark{ remark: vec![i as u8] }.into();
			let call_hash = CallHash(Hashable::blake2_256(&call));
			Votes::<T>::insert(0, call_hash, vec![0]);
		}
	} : { let _ = Votes::<T>::clear_prefix(0, u32::MAX, None); }

	on_idle_with_nothing_to_remove {
		EpochsToCull::<T>::append(1);
	} : { let _ = crate::Pallet::<T>::on_idle(Default::default(), Default::default()); }

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
