//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::benchmarks;
use frame_support::dispatch::UnfilteredDispatchable;

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

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
