//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_primitives::AccountRole;
use cf_traits::AccountRoleRegistry;
use frame_benchmarking::{account, benchmarks, whitelisted_caller};
use frame_support::dispatch::UnfilteredDispatchable;
use frame_system::RawOrigin;

const MAX_VALIDATOR_COUNT: u32 = 150;

benchmarks! {
	update_accrual_ratio {
		let call = Call::<T>::update_accrual_ratio{ reputation_points: 2, online_credits: 151u32.into() };
	} : { let _ = call.dispatch_bypass_filter(T::EnsureGovernance::successful_origin()); }
	set_penalty {
		let call = Call::<T>::set_penalty { offence: PalletOffence::MissedHeartbeat.into(), new_penalty: Default::default() };
	} : { let _ = call.dispatch_bypass_filter(T::EnsureGovernance::successful_origin()); }
	update_missed_heartbeat_penalty {
		let new_reputation_penalty = 20;
		let call = Call::<T>::update_missed_heartbeat_penalty { new_reputation_penalty };
		let heartbeat_block_interval = T::HeartbeatBlockInterval::get();

	} : { let _ = call.dispatch_bypass_filter(T::EnsureGovernance::successful_origin()); }
	verify {
		assert_eq!(
			Pallet::<T>::resolve_penalty_for(PalletOffence::MissedHeartbeat),
			Penalty { reputation: new_reputation_penalty, suspension: heartbeat_block_interval }
		);
	}
	heartbeat {
		let caller: T::AccountId = whitelisted_caller();
		let validator_id: T::ValidatorId = caller.clone().into();
		T::AccountRoleRegistry::register_account(caller.clone(), AccountRole::Validator);
	} : _(RawOrigin::Signed(caller))
	verify {
		assert_eq!(LastHeartbeat::<T>::get(&validator_id), Some(1u32.into()));
	}
	submit_network_state {
		for b in 1 .. MAX_VALIDATOR_COUNT {
			let caller: T::AccountId  = account("doogle", b, b);
			let validator_id: T::ValidatorId = caller.into();
		}
		let interval = T::HeartbeatBlockInterval::get();
		// TODO: set the generated validators as active validators
	} : {
		Pallet::<T>::on_initialize(interval);
	}
	on_initialize_no_action {
		let interval = T::HeartbeatBlockInterval::get();
		let next_block_number = interval + 1u32.into();
	} : {
		Pallet::<T>::on_initialize(next_block_number);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
