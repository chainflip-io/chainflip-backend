use super::*;

use frame_benchmarking::{benchmarks, whitelisted_caller};

benchmarks! {
	register_account_role {
		let caller: T::AccountId = whitelisted_caller();
		Pallet::<T>::on_new_account(&caller);
	}: _(RawOrigin::Signed(caller.clone()), AccountRole::Validator)
	verify {
		assert_eq!(AccountRoles::<T>::get(&caller), Some(AccountRole::Validator));
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
