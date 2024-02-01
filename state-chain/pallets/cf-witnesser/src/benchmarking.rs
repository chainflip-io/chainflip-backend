#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_traits::AccountRoleRegistry;
use frame_benchmarking::v2::*;
use frame_support::traits::{Hooks, OnNewAccount};
use frame_system::RawOrigin;
use sp_std::{boxed::Box, collections::btree_set::BTreeSet};

#[benchmarks]
mod benchmarks {
	use super::*;
	use sp_std::vec;

	#[benchmark]
	fn witness_at_epoch() {
		let caller: T::AccountId = whitelisted_caller();
		let validator_id: T::ValidatorId = caller.clone().into();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		T::AccountRoleRegistry::register_as_validator(&caller).unwrap();
		let call: <T as Config>::RuntimeCall = frame_system::Call::remark { remark: vec![] }.into();
		let epoch = T::EpochInfo::epoch_index();

		T::EpochInfo::add_authority_info_for_epoch(epoch, BTreeSet::from([validator_id]));

		// TODO: currently we don't measure the actual execution path
		// we need to set the threshold to 1 to do this.
		// Unfortunately, this is blocked by the fact that we can't pass
		// a witness call here - for now.
		#[extrinsic_call]
		witness_at_epoch(RawOrigin::Signed(caller.clone()), Box::new(call.clone()), epoch);

		let call_hash = CallHash(Hashable::blake2_256(&call));
		assert!(Votes::<T>::contains_key(epoch, call_hash));
	}

	#[benchmark]
	fn remove_storage_items(n: Linear<1, 255>) {
		for i in 0..n {
			let call: <T as Config>::RuntimeCall =
				frame_system::Call::remark { remark: vec![i as u8] }.into();
			let call_hash = CallHash(Hashable::blake2_256(&call));
			Votes::<T>::insert(0, call_hash, vec![0]);
		}

		#[block]
		{
			let _ = Votes::<T>::clear_prefix(0, u32::MAX, None);
		}
	}

	#[benchmark]
	fn on_idle_with_nothing_to_remove() {
		EpochsToCull::<T>::append(1);

		#[block]
		{
			let _ = crate::Pallet::<T>::on_idle(Default::default(), Default::default());
		}
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
