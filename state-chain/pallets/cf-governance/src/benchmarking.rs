//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use core::convert::TryInto;
use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite, whitelisted_caller};
use frame_support::traits::OnInitialize;
use frame_system::RawOrigin;
use sp_std::{boxed::Box, vec, vec::Vec};

use crate as pallet_cf_governance;
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
	// 	// TODO: use call secured by Governance
	// 	// this is not compiling - but for now i don't have a solution for this
	// 	let call: <T as Config>::Call = pallet_cf_governance::Call::new_membership_set(vec![]).into();
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
		// TODO: mock the time to end in the expire proposals case which is more expensive
		let b in 1 .. 100 as u32;
		for _n in 1 .. b {
			let call = Box::new(frame_system::Call::remark(vec![]).into());
			Governance::<T>::push_proposal(call);
		}
	}: {
		Governance::<T>::on_initialize((b).into());
	}
	on_initialize_best_case {
	}: {
		Governance::<T>::on_initialize((2 as u32).into());
	}
	expire_proposals {
		let b in 1 .. 100 as u32;
		for _n in 1 .. b {
			let call = Box::new(frame_system::Call::remark(vec![]).into());
			Governance::<T>::push_proposal(call);
		}
	} : {
		Governance::<T>::expire_proposals(<ActiveProposals<T>>::get());
	}
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
