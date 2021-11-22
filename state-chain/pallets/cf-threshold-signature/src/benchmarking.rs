//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{
	benchmarks, benchmarks_instance_pallet, impl_benchmark_test_suite, whitelisted_caller,
};

use crate::pallet::SignatureFor;
use cf_chains::eth::SchnorrVerificationComponents;
use frame_support::{
	dispatch::UnfilteredDispatchable,
	traits::{EnsureOrigin, OnInitialize},
};
use frame_system::RawOrigin;

#[allow(unused)]
use crate::Pallet;

benchmarks_instance_pallet! {
	// signature_success {
	// 	let caller: T::AccountId = whitelisted_caller();
	// 	const VALID_SIGNATURE: &'static str = "Wow!";
	// 	let val: Vec<u8> = vec![];
	// 	let sig = SchnorrVerificationComponents {
	// 		s: [0u8; 32],
	// 		k_times_g_addr: [0u8; 20]
	// 	};
	// 	let call = Call::<T, I>::signature_success(1, sig.into());
	// 	let origin = T::EnsureWitnessed::successful_origin();
	// } : { call.dispatch_bypass_filter(origin)? }
	signature_failed {} : {}
	on_initialize {} : {}
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
