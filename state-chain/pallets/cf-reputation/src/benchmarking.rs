//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{account, benchmarks, whitelisted_caller};
use frame_support::dispatch::UnfilteredDispatchable;
use frame_system::RawOrigin;

// TODO: Centralise these constants
const MAX_VALIDATOR_COUNT: u32 = 150;
const HEARTBEAT_BLOCK_INTERVAL: u32 = 150;

benchmarks! {
	update_accrual_ratio {
		let call = Call::<T>::update_accrual_ratio{ points: 2, online_credits: 151u32.into() };
	} : { let _ = call.dispatch_bypass_filter(T::EnsureGovernance::successful_origin()); }
	set_penalty {
		let call = Call::<T>::set_penalty { offence: PalletOffence::MissedHeartbeat.into(), penalty: Default::default() };
	} : { let _ = call.dispatch_bypass_filter(T::EnsureGovernance::successful_origin()); }
	update_missed_heartbeat_penalty {
		let call = Call::<T>::update_missed_heartbeat_penalty { value: ReputationPenaltyRate {
			points: 1,
			per_blocks: (10 as u32).into()
	}};
	} : { let _ = call.dispatch_bypass_filter(T::EnsureGovernance::successful_origin()); }
	verify {
		assert_eq!(
			Pallet::<T>::resolve_penalty_for(PalletOffence::MissedHeartbeat),
			Penalty { reputation: 15, suspension: 0_u32.into() }
		);
	}
	on_runtime_upgrade {
	} : {
		<Pallet::<T> as Hooks<_>>::on_runtime_upgrade();
	}
	heartbeat {
		let caller: T::AccountId = whitelisted_caller();
		let validator_id: T::ValidatorId = caller.clone().into();
	} : _(RawOrigin::Signed(caller))
	verify {
		assert_eq!(LastHeartbeat::<T>::get(&validator_id), Some(1u32.into()));
	}
	submit_network_state {
		for b in 1 .. MAX_VALIDATOR_COUNT {
			let caller: T::AccountId  = account("doogle", b, b);
			let validator_id: T::ValidatorId = caller.into();
		}
		// TODO: set the generated validators as active validators
	} : {
		Pallet::<T>::on_initialize(HEARTBEAT_BLOCK_INTERVAL.into());
	}
	on_initialize_no_action {
	} : {
		Pallet::<T>::on_initialize((HEARTBEAT_BLOCK_INTERVAL + 1).into());
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
