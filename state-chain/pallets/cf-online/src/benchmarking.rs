//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite, whitelisted_caller};
use frame_system::RawOrigin;

#[allow(unused)]
use crate::Pallet as Online;

const HEART_BLOCK_INTERVAL: u32 = 150;
const MAX_VALIDATOR_AMOUNT: u32 = 150;

benchmarks! {
	heartbeat {
		let caller: T::AccountId = whitelisted_caller();
		let validator_id: T::ValidatorId = caller.clone().into();
		let block_number: T::BlockNumber = 1u32.into();
		Nodes::<T>::insert(&validator_id, block_number);
	} : _(RawOrigin::Signed(caller))
	submit_network_state {
		for b in 1 .. MAX_VALIDATOR_AMOUNT {
			let caller: T::AccountId  = account("doogle", b, b);
			let validator_id: T::ValidatorId = caller.into();
			let block_number: T::BlockNumber = 1u32.into();
			Nodes::<T>::insert(&validator_id, block_number);
		}
		// TODO: set the generated validators as active validators
	} : {
		Online::<T>::on_initialize(HEART_BLOCK_INTERVAL.into());
	}
	on_initialize_no_action {
	} : {
		Online::<T>::on_initialize((HEART_BLOCK_INTERVAL + 1).into());
	}
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
