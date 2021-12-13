//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_runtime_benchmark_utilities::BenchmarkDefault;
use frame_benchmarking::{benchmarks, benchmarks_instance_pallet, impl_benchmark_test_suite};
use frame_support::dispatch::UnfilteredDispatchable;
use frame_system::RawOrigin;

#[allow(unused)]
use crate::Pallet;

type SignatureFor<T, I> = <<T as Config<I>>::TargetChain as ChainCrypto>::ThresholdSignature;

benchmarks_instance_pallet! {
	signature_success {
		let ceremony_id = Pallet::<T, I>::request_signature(<T::SigningContext as BenchmarkDefault>::benchmark_default());
		let signature = <SignatureFor<T, I> as BenchmarkDefault>::benchmark_default();
	} : _(RawOrigin::None, ceremony_id, signature)
	signature_failed {} : {}
	on_initialize {} : {}
}

// impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
