#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_chains::benchmarking_value::BenchmarkValue;
use frame_benchmarking::benchmarks_instance_pallet;
use frame_support::{assert_ok, dispatch::UnfilteredDispatchable, traits::EnsureOrigin};

benchmarks_instance_pallet! {
	update_chain_state {
		let call = Call::<T, I>::update_chain_state {
			new_chain_state: ChainState {
				block_height: BenchmarkValue::benchmark_value(),
				tracked_data: BenchmarkValue::benchmark_value(),
			}
		};

		let origin = T::EnsureWitnessed::successful_origin();
		// Dispatch once to ensure we have a value already inserted - replacing a value is more expensive
		// than inserting a new one.
		assert_ok!(call.clone().dispatch_bypass_filter(origin.clone()));
	}: {
		let _ = call.dispatch_bypass_filter(origin);
	} verify {
		assert!(CurrentChainState::<T,I>::get() == ChainState {
			block_height: BenchmarkValue::benchmark_value(),
			tracked_data: BenchmarkValue::benchmark_value(),
		});
	}
}
