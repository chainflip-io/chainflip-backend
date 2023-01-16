//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::{
	dispatch::UnfilteredDispatchable,
	traits::{Get, OnInitialize},
};
use frame_system::RawOrigin;
use sp_std::{boxed::Box, vec};

benchmarks! {
	propose_governance_extrinsic {
		let caller: T::AccountId = whitelisted_caller();
		let call = Box::new(frame_system::Call::remark{remark: vec![]}.into());
		<Members<T>>::put(vec![caller.clone()]);
	}: _(RawOrigin::Signed(caller.clone()), call)
	verify {
		assert_eq!(ProposalIdCounter::<T>::get(), 1);
	}
	approve {
		let call: <T as Config>::RuntimeCall = frame_system::Call::remark{remark: vec![]}.into();
		let caller: T::AccountId = whitelisted_caller();
		<Members<T>>::put(vec![caller.clone()]);
		Pallet::<T>::push_proposal(Box::new(call));
	}: _(RawOrigin::Signed(caller.clone()), 1)
	verify {
		assert_eq!(ProposalIdCounter::<T>::get(), 1);
	}
	new_membership_set {
		let caller: T::AccountId = whitelisted_caller();
		let members = vec![caller];
		let call = Call::<T>::new_membership_set{ accounts: members.clone() };
		let origin = T::EnsureGovernance::successful_origin();
	}: { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(Members::<T>::get(), members);
	}
	call_as_sudo {
		let call: <T as Config>::RuntimeCall = frame_system::Call::set_code_without_checks{ code: vec![1, 2, 3, 4] }.into();
		let sudo_call = Call::<T>::call_as_sudo{ call: Box::new(call) };
		let origin = T::EnsureGovernance::successful_origin();
	}: { sudo_call.dispatch_bypass_filter(origin)? }
	on_initialize {
		// TODO: mock the time to end in the expire proposals case which is more expensive
		let b in 1 .. 100u32;
		for _n in 1 .. b {
			let call = Box::new(frame_system::Call::remark{remark: vec![]}.into());
			Pallet::<T>::push_proposal(call);
		}
	}: {
		Pallet::<T>::on_initialize(2u32.into());
	}
	on_initialize_best_case {
	}: {
		Pallet::<T>::on_initialize(2u32.into());
	}
	expire_proposals {
		let b in 1 .. 100u32;
		for _n in 1 .. b {
			let call = Box::new(frame_system::Call::remark{remark: vec![]}.into());
			Pallet::<T>::push_proposal(call);
		}
	} : {
		Pallet::<T>::expire_proposals(<ActiveProposals<T>>::get());
	}
	set_whitelisted_call_hash {
		let call_hash = [0xb; 32];

		let call = Call::<T>::set_whitelisted_call_hash{
			call_hash,
		};

	} : {
		call.dispatch_bypass_filter(T::EnsureWitnessedAtCurrentEpoch::successful_origin())?;
	}
	verify {
		assert_eq!(GovKeyWhitelistedCallHash::<T>::get().unwrap(), call_hash);
	}
	submit_govkey_call {
		let next_nonce = 788;
		NextGovKeyCallHashNonce::<T>::put(next_nonce);

		let new_membership_set_call: <T as Config>::RuntimeCall = Call::<T>::new_membership_set {
			accounts: vec![]
		}.into();

		let call_hash = frame_support::Hashable::blake2_256(&(
			new_membership_set_call.clone(),
			next_nonce,
			T::Version::get(),
		));

		GovKeyWhitelistedCallHash::<T>::put(call_hash);

		let call = Call::<T>::submit_govkey_call {
			call: Box::new(new_membership_set_call),
		};
	} : {
		call.dispatch_bypass_filter(T::EnsureGovernance::successful_origin())?;
	}
	verify {
		assert_eq!(NextGovKeyCallHashNonce::<T>::get(), next_nonce + 1);
		assert!(GovKeyWhitelistedCallHash::<T>::get().is_none());
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
