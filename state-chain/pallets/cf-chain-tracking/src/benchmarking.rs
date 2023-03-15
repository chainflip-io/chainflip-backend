use super::*;

use cf_chains::BenchmarkValue;
use frame_benchmarking::benchmarks_instance_pallet;
use frame_support::{assert_ok, dispatch::UnfilteredDispatchable, traits::EnsureOrigin};

benchmarks_instance_pallet! {
	update_chain_state {
		let call = Call::<T, I>::update_chain_state {
			state: BenchmarkValue::benchmark_value()
		};

		let origin = T::EnsureWitnessed::successful_origin();
		// Dispatch once to ensure we have a value already inserted - replacing a value is more expensive
		// than inserting a new one.
		assert_ok!(call.clone().dispatch_bypass_filter(origin.clone()));
	}: {
		let _ = call.dispatch_bypass_filter(origin);
	} verify {
		assert!(ChainState::<T,I>::get() == Some(BenchmarkValue::benchmark_value()));
	}
}
