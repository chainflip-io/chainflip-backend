//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use core::convert::TryInto;
use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite, whitelisted_caller};
use frame_support::traits::OnInitialize;
use frame_system::RawOrigin;
use sp_std::{boxed::Box, vec, vec::Vec};

#[allow(unused)]
use crate::Pallet as Governance;

benchmarks! {
	propose_governance_extrinsic {
		let caller: T::AccountId = whitelisted_caller();
		let call = Box::new(frame_system::Call::remark(vec![]).into());
		<Members<T>>::put(vec![caller.clone()]);
	}: _(RawOrigin::Signed(caller.clone()), call)
	approve {
		let call: <T as Config>::Call = frame_system::Call::remark(vec![]).into();
		let caller: T::AccountId = whitelisted_caller();
		<Members<T>>::put(vec![caller.clone()]);
		Governance::<T>::push_proposal(Box::new(call));
	}: _(RawOrigin::Signed(caller.clone()), 1)
	new_membership_set {
		let caller: T::AccountId = whitelisted_caller();
		let members = vec![caller.clone()];
		let call = Call::<T>::new_membership_set(members);
		let origin = T::EnsureGovernance::successful_origin();
	}: { call.dispatch_bypass_filter(origin)? }
	// execute {
	// 	let caller: T::AccountId = whitelisted_caller();
	// 	let members = vec![caller.clone()];
	//  // TODO: use call secured by Governance
	// 	// let call = Call::<T>::new_membership_set(members);
	// 	let call = pallet::Call::<T>::new_membership_set(members);
	// 	<Members<T>>::put(members);
	// 	let id = Governance::<T>::push_proposal(Box::new(call));
	// 	Governance::<T>::try_approve(caller.clone(), id);
	// }: _(RawOrigin::Signed(caller.clone()), id)
	call_as_sudo {
		let call: <T as Config>::Call = frame_system::Call::set_code_without_checks(vec![1, 2, 3, 4]).into();
		let sudo_call = Call::<T>::call_as_sudo(Box::new(call));
		let origin = T::EnsureGovernance::successful_origin();
	}: { sudo_call.dispatch_bypass_filter(origin)? }
	on_initialize {
		//TODO: set the time
		for _n in 1..100 {
			let call = Box::new(frame_system::Call::remark(vec![]).into());
			Governance::<T>::push_proposal(call);
		}
	}: {
		Governance::<T>::on_initialize((2 as u32).into());
	}
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
