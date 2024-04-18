#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_chains::benchmarking_value::BenchmarkValue;
use frame_benchmarking::v2::*;
use frame_support::{
	assert_ok,
	traits::{EnsureOrigin, UnfilteredDispatchable},
};

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn update_chain_state() {
		let genesis_chain_state = ChainState {
			block_height: 1u32.into(),
			tracked_data: BenchmarkValue::benchmark_value(),
		};
		let new_chain_state = ChainState {
			block_height: 32u32.into(),
			tracked_data: BenchmarkValue::benchmark_value(),
		};

		let origin = T::EnsureWitnessed::try_successful_origin().unwrap();
		// Dispatch once to ensure we have a value already inserted - replacing a value is more
		// expensive than inserting a new one.
		assert_ok!(Call::<T, I>::update_chain_state { new_chain_state: genesis_chain_state }
			.dispatch_bypass_filter(origin.clone()));

		#[block]
		{
			assert_ok!(Call::<T, I>::update_chain_state {
				new_chain_state: new_chain_state.clone()
			}
			.dispatch_bypass_filter(origin));
		}

		assert_eq!(CurrentChainState::<T, I>::get().unwrap(), new_chain_state);
	}

	#[cfg(test)]
	use crate::mock::*;

	#[test]
	fn benchmark_works() {
		new_test_ext().execute_with(|| {
			_update_chain_state::<Test, ()>(true);
		});
	}
}
