//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite, whitelisted_caller};
use frame_system::RawOrigin;

const HEART_BLOCK_INTERVAL: u32 = 150;
const MAX_VALIDATOR_AMOUNT: u32 = 150;

benchmarks! {
	heartbeat {
		let caller: T::AccountId = whitelisted_caller();
		let validator_id: T::ValidatorId = caller.clone().into();
	} : _(RawOrigin::Signed(caller))
	verify {
		assert_eq!(LastHeartbeat::<T>::get(&validator_id), Some(1u32.into()));
	}
	submit_network_state {
		for b in 1 .. MAX_VALIDATOR_AMOUNT {
			let caller: T::AccountId  = account("doogle", b, b);
			let validator_id: T::ValidatorId = caller.into();
		}
		// TODO: set the generated validators as active validators
	} : {
		Pallet::<T>::on_initialize(HEART_BLOCK_INTERVAL.into());
	}
	on_initialize_no_action {
	} : {
		Pallet::<T>::on_initialize((HEART_BLOCK_INTERVAL + 1).into());
	}
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
