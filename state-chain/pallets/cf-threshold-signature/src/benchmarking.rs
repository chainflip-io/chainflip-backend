//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{benchmarks_instance_pallet, impl_benchmark_test_suite};

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
	signature_success {} : {}
	signature_failed {} : {}
	on_initialize {} : {}
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
