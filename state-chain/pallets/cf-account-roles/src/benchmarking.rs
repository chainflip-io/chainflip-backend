#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::v2::*;
use frame_support::{assert_ok, traits::UnfilteredDispatchable};

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn enable_swapping() {
		let origin = <T as Config>::EnsureGovernance::try_successful_origin().unwrap();
		let call = Call::<T>::enable_swapping {};

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin));
		}

		assert!(SwappingEnabled::<T>::get());
	}

	#[benchmark]
	fn gov_register_account_role() {
		let origin = <T as Config>::EnsureGovernance::try_successful_origin().unwrap();
		let caller: T::AccountId = whitelisted_caller();
		Pallet::<T>::on_new_account(&caller);
		let call = Call::<T>::gov_register_account_role {
			account: caller.clone(),
			role: AccountRole::Broker,
		};

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin));
		}

		assert_eq!(AccountRoles::<T>::get(&caller), Some(AccountRole::Broker));
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
