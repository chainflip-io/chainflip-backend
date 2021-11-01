//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite, whitelisted_caller};
use frame_system::RawOrigin;
use sp_std::{boxed::Box, vec, vec::Vec};

#[allow(unused)]
use crate::Pallet as Online;

const HEART_BLOCK_INTERVAL: u32 = 150;
const MAX_VALIDATOR_AMOUNT: u32 = 150;

benchmarks! {
	heartbeat {
		let caller: T::AccountId = whitelisted_caller();
		let validator_id: T::ValidatorId = caller.clone().into();
		Nodes::<T>::insert(&validator_id, 2);
	} : _(RawOrigin::Signed(caller))
	submit_network_state {
		let x in 1 .. 1_000;
		for i in 1 .. MAX_VALIDATOR_AMOUNT {
			let caller: T::AccountId  = account("doogle", i, i);
			let validator_id: T::ValidatorId = caller.into();
			Nodes::<T>::insert(&validator_id, 2);
		}
		// TODO: set the generated validators as active validators
	} : {
		for b in 1..x {
			if b % HEART_BLOCK_INTERVAL == 0 {
				Online::<T>::on_initialize((b as u32).into());
			}
		}
	}
	on_initialize_no_action {
		let x in 1 .. 1_000;
	} : {
		for b in 1..x {
			if b % HEART_BLOCK_INTERVAL != 0 {
				Online::<T>::on_initialize((b as u32).into());
			}
		}
	}
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
